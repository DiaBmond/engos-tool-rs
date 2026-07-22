use std::time::Duration;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use serde_json::Value;

use crate::application::deps::AppDeps;
use crate::application::messaging::ports::MessagingPort;
use crate::application::roleplay::dto::RoleplayScenario;
use crate::application::roleplay::ports::RoleplayUseCase;
use crate::application::sentence::ports::SentenceUseCase;
use crate::application::session::ports::{ChatStateRepository, LockToken, SessionLockRepository};
use crate::application::user::ports::UserUseCase;
use crate::application::vocab::ports::VocabUseCase;
use crate::domain::chat_state::{
    ChatState, MAX_VOCAB_ATTEMPTS, ROLEPLAY_TOTAL_TURNS, RoleplayTurn, STATE_TTL_SECONDS,
    VOCAB_ROUND_SIZE,
};
use crate::domain::error::{AppError, AppResult};
use crate::domain::user::{STACK_TO_LEVEL_UP, User};
use crate::infrastructure::http::signature::verify_line_signature;

/// Hard ceiling on one turn's processing time.
///
/// Bounded so that a turn can never outlive the lock protecting it. Without
/// this, a slow AI call plus both LINE attempts could exceed the lock TTL, the
/// lock would expire mid-turn, and a second message could interleave — exactly
/// the lost update the lock exists to prevent.
const TURN_DEADLINE: Duration = Duration::from_secs(45);

/// How long one user's turn may hold the lock.
const LOCK_TTL_SECONDS: u64 = 90;

/// The invariant that makes the lock meaningful, checked at compile time so a
/// future edit to either constant cannot silently break it.
const _: () = assert!(
    TURN_DEADLINE.as_secs() < LOCK_TTL_SECONDS,
    "a turn must finish before its lock can expire"
);

/// Brief retry when another message from the same learner is mid-flight.
/// Absorbs ordinary double-sends without making the learner retype.
const LOCK_RETRY_ATTEMPTS: u32 = 8;
const LOCK_RETRY_DELAY: Duration = Duration::from_millis(250);

/// Retention for processed webhook event ids. LINE retries well inside this.
const EVENT_DEDUP_TTL_SECONDS: u64 = 600;

const MENU: &str = "พิมพ์ตัวเลขเพื่อเลือกโหมดฝึก:\n1. ทายศัพท์ (Vocab)\n2. ทบทวนศัพท์ (Review)\n3. แต่งประโยค (Sentence)\n4. โรลเพลย์ (Roleplay)";

/// Webhook entry point.
///
/// Takes the raw body rather than `Json<Value>`: LINE signs the exact bytes it
/// sent, and round-tripping through `serde_json::Value` would reorder keys and
/// invalidate the signature.
///
/// Returns `200` as soon as the payload is authenticated and parsed. Everything
/// slow runs in a spawned task, because LINE times out after roughly ten
/// seconds and retries.
pub async fn handle_webhook<D: AppDeps>(
    State(deps): State<D>,
    headers: HeaderMap,
    body: Bytes,
) -> StatusCode {
    let signature = headers
        .get("x-line-signature")
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default();

    if !verify_line_signature(deps.line_channel_secret(), &body, signature) {
        tracing::warn!(
            has_signature = !signature.is_empty(),
            body_len = body.len(),
            "rejected webhook with invalid signature"
        );
        return StatusCode::UNAUTHORIZED;
    }

    let payload: Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "rejected webhook with unparseable body");
            return StatusCode::BAD_REQUEST;
        }
    };

    let Some(events) = payload.get("events").and_then(Value::as_array) else {
        // A verified payload with no events is normal (LINE's "Verify" button).
        return StatusCode::OK;
    };

    for event in events {
        if let Some(message) = TextMessage::from_event(event) {
            let deps = deps.clone();
            tokio::spawn(async move {
                process_event(deps, message).await;
            });
        }
    }

    StatusCode::OK
}

/// The subset of a LINE event this bot acts on.
#[derive(Debug, Clone)]
struct TextMessage {
    event_id: Option<String>,
    user_id: String,
    reply_token: String,
    text: String,
}

impl TextMessage {
    fn from_event(event: &Value) -> Option<Self> {
        if event.get("type")?.as_str()? != "message" {
            return None;
        }
        if event.get("message")?.get("type")?.as_str()? != "text" {
            return None;
        }

        let user_id = event
            .get("source")?
            .get("userId")?
            .as_str()
            .filter(|s| !s.is_empty())?
            .to_string();

        let reply_token = event
            .get("replyToken")?
            .as_str()
            .filter(|s| !s.is_empty())?
            .to_string();

        let text = event.get("message")?.get("text")?.as_str()?.trim();
        if text.is_empty() {
            return None;
        }

        Some(Self {
            event_id: event
                .get("webhookEventId")
                .and_then(Value::as_str)
                .map(str::to_string),
            user_id,
            reply_token,
            text: text.to_string(),
        })
    }
}

/// Runs one event to completion: deduplication, locking, deadline enforcement,
/// panic containment and error reporting.
///
/// Never returns an error — it is the top of a spawned task, so anything
/// unhandled here would simply vanish.
#[tracing::instrument(
    skip_all,
    fields(user_id = %message.user_id, event_id = message.event_id.as_deref().unwrap_or("-"))
)]
async fn process_event<D: AppDeps>(deps: D, message: TextMessage) {
    if is_duplicate(&deps, &message).await {
        return;
    }

    let lock = match acquire_lock(&deps, &message.user_id).await {
        Ok(Some(lock)) => Some(lock),
        Ok(None) => {
            tracing::info!("user turn still in progress after retries, asking them to resend");
            let _ = deps
                .messaging()
                .respond(
                    &message.reply_token,
                    &message.user_id,
                    "ยังตอบข้อความก่อนหน้าไม่เสร็จครับ 🤔\nรอสักครู่แล้วส่งข้อความนี้ใหม่อีกครั้งนะครับ",
                )
                .await;
            return;
        }
        Err(error) => {
            tracing::warn!(%error, "lock unavailable, proceeding unlocked");
            None
        }
    };

    let result = run_turn(deps.clone(), message.clone()).await;

    if let Err(error) = &result {
        tracing::error!(
            %error,
            kind = error.kind(),
            transient = error.is_transient(),
            "failed to process message"
        );
        let _ = deps
            .messaging()
            .respond(&message.reply_token, &message.user_id, error.user_message())
            .await;
    }

    // Runs on every path, including panics and deadline overruns, so a lock is
    // never held longer than the work it guards.
    if let Some(lock) = lock
        && let Err(error) = deps.session().release_lock(&lock).await
    {
        tracing::warn!(%error, "failed to release user lock");
    }
}

/// Suppresses LINE's retries of an event already handled.
async fn is_duplicate<D: AppDeps>(deps: &D, message: &TextMessage) -> bool {
    let Some(event_id) = message.event_id.as_deref() else {
        return false;
    };

    match deps
        .session()
        .try_claim_event(event_id, EVENT_DEDUP_TTL_SECONDS)
        .await
    {
        Ok(false) => {
            tracing::info!("skipping duplicate webhook delivery");
            true
        }
        Ok(true) => false,
        Err(error) => {
            // Redis being down should not silence the bot; proceed and accept
            // the small risk of double processing.
            tracing::warn!(%error, "event deduplication unavailable");
            false
        }
    }
}

/// Takes the per-user lock, retrying briefly rather than immediately giving up.
///
/// Learners routinely send two messages in quick succession; rejecting the
/// second outright meant discarding what they had typed.
async fn acquire_lock<D: AppDeps>(deps: &D, user_id: &str) -> AppResult<Option<LockToken>> {
    for attempt in 0..LOCK_RETRY_ATTEMPTS {
        if let Some(lock) = deps
            .session()
            .try_acquire_lock(user_id, LOCK_TTL_SECONDS)
            .await?
        {
            return Ok(Some(lock));
        }

        if attempt + 1 < LOCK_RETRY_ATTEMPTS {
            tokio::time::sleep(LOCK_RETRY_DELAY).await;
        }
    }

    Ok(None)
}

/// Executes the turn under a deadline, containing panics.
///
/// A panic in a spawned task would otherwise bypass tracing entirely and leave
/// the lock held until its TTL expired.
async fn run_turn<D: AppDeps>(deps: D, message: TextMessage) -> AppResult<()> {
    let mut handle = tokio::spawn(async move { process_user_message(&deps, &message).await });

    match tokio::time::timeout(TURN_DEADLINE, &mut handle).await {
        Ok(Ok(result)) => result,
        Ok(Err(join_error)) if join_error.is_panic() => Err(AppError::Internal(format!(
            "handler panicked: {join_error}"
        ))),
        Ok(Err(join_error)) => Err(AppError::Internal(join_error.to_string())),
        Err(_elapsed) => {
            // Abort rather than detach: a task left running past the deadline
            // would write conversation state after the lock had been released.
            handle.abort();
            Err(AppError::Timeout(TURN_DEADLINE.as_secs()))
        }
    }
}

async fn process_user_message<D: AppDeps>(deps: &D, message: &TextMessage) -> AppResult<()> {
    let mut user = deps.users().get_or_create(&message.user_id).await?;
    let current_state = deps.session().get_state(&message.user_id).await?;

    tracing::info!(
        state = current_state.name(),
        level = user.current_level,
        "handling message"
    );

    if is_exit_command(&message.text) {
        deps.session().clear_state(&message.user_id).await?;
        return reply(deps, message, &format!("ออกสู่เมนูหลักแล้วครับ\n\n{MENU}")).await;
    }

    match current_state {
        ChatState::Idle => handle_idle(deps, &user, message).await,
        ChatState::VocabGuessing {
            vocab_ids,
            current_index,
            attempt,
        } => {
            handle_vocab_guessing(deps, &mut user, message, vocab_ids, current_index, attempt).await
        }
        ChatState::VocabReviewing {
            review_list,
            current_index,
        } => handle_vocab_reviewing(deps, message, review_list, current_index).await,
        ChatState::SentenceDraft {
            sentence_id,
            original_text,
            fix_count,
        } => handle_sentence_draft(deps, message, sentence_id, original_text, fix_count).await,
        ChatState::Roleplay {
            turn_count,
            scenario,
            history,
        } => handle_roleplay(deps, &mut user, message, turn_count, scenario, history).await,
    }
}

fn is_exit_command(text: &str) -> bool {
    let normalized = text.trim().to_lowercase();
    matches!(normalized.as_str(), "ยกเลิก" | "ออก" | "exit" | "cancel")
}

/// Sends a reply, falling back to a push if the reply token has expired.
async fn reply<D: AppDeps>(deps: &D, message: &TextMessage, text: &str) -> AppResult<()> {
    deps.messaging()
        .respond(&message.reply_token, &message.user_id, text)
        .await
}

async fn set_state<D: AppDeps>(deps: &D, user_id: &str, next: &ChatState) -> AppResult<()> {
    deps.session()
        .set_state(user_id, next, STATE_TTL_SECONDS)
        .await
}

/// Reads an index out of a state-held list, reporting a stale state rather than
/// panicking.
fn at<'a>(list: &'a [String], index: usize, what: &str) -> AppResult<&'a str> {
    list.get(index).map(String::as_str).ok_or_else(|| {
        AppError::InvalidState(format!(
            "{what} index {index} out of range for {} entries",
            list.len()
        ))
    })
}

// ---------------------------------------------------------------------------
// Idle: mode selection
// ---------------------------------------------------------------------------

async fn handle_idle<D: AppDeps>(deps: &D, user: &User, message: &TextMessage) -> AppResult<()> {
    match message.text.to_lowercase().as_str() {
        "1" | "ทายศัพท์" | "vocab" => start_vocab_round(deps, user, message).await,
        "2" | "ทบทวนศัพท์" | "review" => {
            start_vocab_review(deps, user, message).await
        }
        "3" | "แต่งประโยค" | "sentence" => {
            start_sentence_draft(deps, user, message).await
        }
        "4" | "โรลเพลย์" | "roleplay" => start_roleplay(deps, user, message).await,
        _ => {
            reply(
                deps,
                message,
                &format!("ยินดีต้อนรับสู่ EngOS! 🚀 ระบบอัปสกิลภาษาอังกฤษสำหรับโปรแกรมเมอร์\n\n{MENU}"),
            )
            .await
        }
    }
}

async fn start_vocab_round<D: AppDeps>(
    deps: &D,
    user: &User,
    message: &TextMessage,
) -> AppResult<()> {
    let vocabs = deps.vocab().start_new_round(&user.user_id).await?;

    let Some(first) = vocabs.first() else {
        return reply(deps, message, "ตอนนี้ยังสร้างคำศัพท์ไม่ได้ครับ ลองใหม่อีกครั้งนะครับ 🙏").await;
    };

    let prompt = format!(
        "🔥 โหมดทายคำศัพท์เริ่มแล้ว! (ข้อที่ 1/{})\n\n💡 คำแปล: \"{}\"\n📂 หมวดหมู่: {}\n\n👉 พิมพ์คำศัพท์ภาษาอังกฤษส่งมาได้เลยครับ!",
        vocabs.len(),
        first.definition,
        first.category
    );

    let vocab_ids: Vec<String> = vocabs.iter().map(|v| v.vocab_id.clone()).collect();

    set_state(
        deps,
        &user.user_id,
        &ChatState::VocabGuessing {
            vocab_ids,
            current_index: 0,
            attempt: 1,
        },
    )
    .await?;

    reply(deps, message, &prompt).await
}

async fn start_vocab_review<D: AppDeps>(
    deps: &D,
    user: &User,
    message: &TextMessage,
) -> AppResult<()> {
    let review_data = deps.vocab().get_review_vocabs(&user.user_id).await?;

    // An empty library is an ordinary situation, not a failure. It used to be
    // returned as `Err`, so a new learner saw a system-error message here.
    let Some((first_vocab, _)) = review_data.first() else {
        return reply(
            deps,
            message,
            "ยังไม่มีคำศัพท์ให้ทบทวนครับ ลองเล่นโหมด \"1. ทายศัพท์\" ก่อนนะครับ!",
        )
        .await;
    };

    let prompt = format!(
        "🔄 โหมดทบทวนศัพท์เก่า (ข้อที่ 1/{})\n\n💡 คำแปล: \"{}\"\n📂 หมวดหมู่: {}\n\n👉 พิมพ์คำศัพท์ภาษาอังกฤษที่คุณจำได้ส่งมาเลยครับ!",
        review_data.len(),
        first_vocab.definition,
        first_vocab.category
    );

    let review_list: Vec<String> = review_data
        .into_iter()
        .map(|(vocab, _)| vocab.vocab_id)
        .collect();

    set_state(
        deps,
        &user.user_id,
        &ChatState::VocabReviewing {
            review_list,
            current_index: 0,
        },
    )
    .await?;

    reply(deps, message, &prompt).await
}

async fn start_sentence_draft<D: AppDeps>(
    deps: &D,
    user: &User,
    message: &TextMessage,
) -> AppResult<()> {
    set_state(
        deps,
        &user.user_id,
        &ChatState::SentenceDraft {
            sentence_id: None,
            original_text: None,
            fix_count: 0,
        },
    )
    .await?;

    reply(
        deps,
        message,
        "✍️ โหมดฝึกแต่งประโยค\n\nพิมพ์ประโยคภาษาอังกฤษอะไรก็ได้ส่งมาเลยครับ AI จะช่วยตรวจและแนะทริคให้โดยไม่เฉลยคำตอบตรงๆ!",
    )
    .await
}

async fn start_roleplay<D: AppDeps>(deps: &D, user: &User, message: &TextMessage) -> AppResult<()> {
    let scenario = deps.roleplay().start_new_session(user).await?;

    let prompt = format!(
        "🎭 โหมดสวมบทบาท (Level {})\n📌 สถานการณ์: {}\n🤖 บทบาท AI: {}\n\n💬 AI เริ่มคุย:\n\"{}\"\n\n👉 พิมพ์ตอบกลับเป็นภาษาอังกฤษเพื่อเริ่มได้เลยครับ! (ทั้งหมด {ROLEPLAY_TOTAL_TURNS} เทิร์น)",
        user.current_level, scenario.setting, scenario.role_name, scenario.opening_line
    );

    set_state(
        deps,
        &user.user_id,
        &ChatState::Roleplay {
            turn_count: 1,
            scenario,
            history: Vec::new(),
        },
    )
    .await?;

    reply(deps, message, &prompt).await
}

// ---------------------------------------------------------------------------
// Vocab guessing
// ---------------------------------------------------------------------------

async fn handle_vocab_guessing<D: AppDeps>(
    deps: &D,
    user: &mut User,
    message: &TextMessage,
    vocab_ids: Vec<String>,
    current_index: usize,
    attempt: u8,
) -> AppResult<()> {
    let current_id = at(&vocab_ids, current_index, "vocab")?;

    let vocab = deps.vocab().get_vocab(current_id).await?;
    let evaluation = deps.vocab().check_answer(&vocab, &message.text).await?;

    deps.vocab()
        .record_answer(&user.user_id, &vocab.vocab_id, evaluation.is_correct)
        .await?;

    let out_of_attempts = attempt >= MAX_VOCAB_ATTEMPTS;

    if !evaluation.is_correct && !out_of_attempts {
        set_state(
            deps,
            &user.user_id,
            &ChatState::VocabGuessing {
                vocab_ids,
                current_index,
                // Saturating: a determined learner could otherwise wrap this
                // counter and reset their own attempt budget.
                attempt: attempt.saturating_add(1),
            },
        )
        .await?;

        return reply(
            deps,
            message,
            &format!(
                "❌ ยังไม่ใช่ครับ! (ทายข้อนี้ไปแล้ว {attempt}/{MAX_VOCAB_ATTEMPTS} ครั้ง)\n💡 คำใบ้จาก AI: {}\n\n👉 ลองเดาใหม่อีกครั้งได้เลยครับ!",
                evaluation.feedback
            ),
        )
        .await;
    }

    let header = if evaluation.is_correct {
        format!(
            "✅ ถูกต้องยอดเยี่ยมครับ!\n🎯 คำศัพท์คือ: \"{}\"\n⭐ Feedback: {}",
            vocab.word, evaluation.feedback
        )
    } else {
        format!(
            "😅 หมดสิทธิ์ทายข้อนี้แล้วครับ!\n🎯 เฉลย: \"{}\" — {}\n⭐ Feedback: {}",
            vocab.word, vocab.definition, evaluation.feedback
        )
    };

    let next_index = current_index + 1;
    let total = vocab_ids.len();

    if next_index < total {
        let next_id = at(&vocab_ids, next_index, "vocab")?;
        let next_vocab = deps.vocab().get_vocab(next_id).await?;

        let body = format!(
            "{header}\n\n------------------\n🔥 คำศัพท์ข้อต่อไป (ข้อที่ {}/{total})\n💡 คำแปล: \"{}\"\n📂 หมวดหมู่: {}\n\n👉 พิมพ์คำทายส่งมาเลยครับ!",
            next_index + 1,
            next_vocab.definition,
            next_vocab.category
        );

        set_state(
            deps,
            &user.user_id,
            &ChatState::VocabGuessing {
                vocab_ids,
                current_index: next_index,
                attempt: 1,
            },
        )
        .await?;

        return reply(deps, message, &body).await;
    }

    // Round finished. Progress goes through the shared domain rule so vocab and
    // roleplay cannot drift apart.
    let levelled_up = deps.users().award_progress(user).await?;
    deps.session().clear_state(&user.user_id).await?;

    let summary = if levelled_up {
        format!(
            "🎉 ยินดีด้วยครับ! LEVEL UP เป็น Level {} แล้ว!",
            user.current_level
        )
    } else if user.is_max_level() {
        "🏅 คุณอยู่ระดับสูงสุดแล้วครับ!".to_string()
    } else {
        format!(
            "📊 แต้มสะสม: {}/{} (อีก {} รอบจะเลเวลอัป)",
            user.progress_stack,
            STACK_TO_LEVEL_UP,
            user.progress_remaining()
        )
    };

    reply(
        deps,
        message,
        &format!(
            "{header}\n\n🏆 ทายคำศัพท์ครบ {VOCAB_ROUND_SIZE} ข้อเรียบร้อยแล้วครับ!\n{summary}\n\n{MENU}"
        ),
    )
    .await
}

// ---------------------------------------------------------------------------
// Vocab review
// ---------------------------------------------------------------------------

async fn handle_vocab_reviewing<D: AppDeps>(
    deps: &D,
    message: &TextMessage,
    review_list: Vec<String>,
    current_index: usize,
) -> AppResult<()> {
    let current_id = at(&review_list, current_index, "review")?;

    let vocab = deps.vocab().get_vocab(current_id).await?;
    let evaluation = deps.vocab().check_answer(&vocab, &message.text).await?;

    // This is what makes review ordering adapt over time; the call existed
    // before but was never wired up, so reviewing changed nothing.
    deps.vocab()
        .record_answer(&message.user_id, &vocab.vocab_id, evaluation.is_correct)
        .await?;

    let header = if evaluation.is_correct {
        format!(
            "✅ ถูกต้องครับ! คำศัพท์คือ \"{}\"\n⭐ Feedback: {}",
            vocab.word, evaluation.feedback
        )
    } else {
        format!(
            "❌ ยังไม่ถูกครับ คำตอบคือ \"{}\"\n⭐ Feedback: {}",
            vocab.word, evaluation.feedback
        )
    };

    let next_index = current_index + 1;
    let total = review_list.len();

    if next_index >= total {
        deps.session().clear_state(&message.user_id).await?;
        return reply(
            deps,
            message,
            &format!("{header}\n\n🎉 ทบทวนคำศัพท์ครบทุกข้อแล้วครับ เก่งมาก!\n\n{MENU}"),
        )
        .await;
    }

    let next_id = at(&review_list, next_index, "review")?;
    let next_vocab = deps.vocab().get_vocab(next_id).await?;

    let body = format!(
        "{header}\n\n------------------\n🔄 คำศัพท์คำต่อไป (ข้อที่ {}/{total})\n💡 คำแปล: \"{}\"\n📂 หมวดหมู่: {}\n\n👉 พิมพ์คำทายส่งมาเลยครับ!",
        next_index + 1,
        next_vocab.definition,
        next_vocab.category
    );

    set_state(
        deps,
        &message.user_id,
        &ChatState::VocabReviewing {
            review_list,
            current_index: next_index,
        },
    )
    .await?;

    reply(deps, message, &body).await
}

// ---------------------------------------------------------------------------
// Sentence drafting
// ---------------------------------------------------------------------------

async fn handle_sentence_draft<D: AppDeps>(
    deps: &D,
    message: &TextMessage,
    sentence_id: Option<String>,
    original_text: Option<String>,
    fix_count: u8,
) -> AppResult<()> {
    let sentence_id = sentence_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    let outcome = deps
        .sentences()
        .submit_draft(
            &sentence_id,
            &message.user_id,
            &message.text,
            original_text.as_deref(),
            fix_count,
        )
        .await?;

    if outcome.analysis.is_passed {
        deps.session().clear_state(&message.user_id).await?;
        return reply(
            deps,
            message,
            &format!(
                "✅ ยอดเยี่ยมมากครับ! ประโยคผ่านเรียบร้อย (แก้ไขไป {} ครั้ง)\n\n💡 Native Trick สำหรับคุณ:\n{}\n\n{MENU}",
                outcome.total_fix, outcome.analysis.feedback
            ),
        )
        .await;
    }

    set_state(
        deps,
        &message.user_id,
        &ChatState::SentenceDraft {
            sentence_id: Some(sentence_id),
            // Carrying the first draft forward is what lets the stored row
            // record what the learner originally wrote.
            original_text: Some(outcome.original_text),
            fix_count: outcome.total_fix,
        },
    )
    .await?;

    reply(
        deps,
        message,
        &format!(
            "🧐 โครงสร้างยังไม่เป๊ะครับ! (แก้ไขไปแล้ว {} ครั้ง)\n\n💡 คำใบ้จาก AI Coach:\n{}\n\n👉 ลองปรับประโยคแล้วส่งมาใหม่อีกครั้งครับ!",
            outcome.total_fix, outcome.analysis.feedback
        ),
    )
    .await
}

// ---------------------------------------------------------------------------
// Roleplay
// ---------------------------------------------------------------------------

async fn handle_roleplay<D: AppDeps>(
    deps: &D,
    user: &mut User,
    message: &TextMessage,
    turn_count: u8,
    scenario: RoleplayScenario,
    mut history: Vec<RoleplayTurn>,
) -> AppResult<()> {
    if turn_count < ROLEPLAY_TOTAL_TURNS {
        let turn = deps
            .roleplay()
            .handle_turn(&scenario, &history, &message.text)
            .await?;

        history.push(RoleplayTurn {
            user_message: message.text.clone(),
            ai_message: turn.ai_message.clone(),
        });

        let mut body = format!(
            "💬 [Turn {turn_count}/{ROLEPLAY_TOTAL_TURNS}] AI Roleplay:\n\"{}\"",
            turn.ai_message
        );

        if !turn.is_understood {
            body.push_str("\n\n⚠️ ประโยคเมื่อกี้ AI ยังไม่ค่อยเข้าใจครับ ลองเรียบเรียงใหม่ดูนะครับ");
        }

        if let Some(hint) = turn.hint.as_deref() {
            body.push_str(&format!("\n\n💡 คำใบ้ตอบเทิร์นถัดไป: {hint}"));
        }

        body.push_str("\n\n👉 พิมพ์ตอบกลับเป็นภาษาอังกฤษได้เลยครับ!");

        set_state(
            deps,
            &user.user_id,
            &ChatState::Roleplay {
                turn_count: turn_count.saturating_add(1),
                scenario,
                history,
            },
        )
        .await?;

        return reply(deps, message, &body).await;
    }

    // Final turn: record it, then grade the whole session.
    history.push(RoleplayTurn {
        user_message: message.text.clone(),
        ai_message: String::new(),
    });

    let evaluation = deps.roleplay().grade_session(&scenario, &history).await?;

    // Progression is applied here, through the same path every other mode uses.
    let levelled_up = if evaluation.is_passed {
        deps.users().award_progress(user).await?
    } else {
        deps.users().penalize(user).await?;
        false
    };

    deps.session().clear_state(&user.user_id).await?;

    let status = if levelled_up {
        format!(
            "🎉 ยินดีด้วยครับ! คุณสะสมครบและ LEVEL UP เป็น Level {} สำเร็จ!",
            user.current_level
        )
    } else if evaluation.is_passed {
        format!(
            "💪 สอบผ่านประจำรอบ! (แต้มสะสม: {}, อีก {} รอบจะเลเวลอัป)",
            user.progress_stack,
            user.progress_remaining()
        )
    } else {
        format!("❌ รอบนี้ยังไม่ผ่านเกณฑ์ครับ (แต้มสะสม: {})", user.progress_stack)
    };

    reply(
        deps,
        message,
        &format!(
            "🏁 จบเซสชันโรลเพลย์!\n📌 {status}\n📊 ระดับปัจจุบัน: Level {}\n\n📋 สรุปผลการประเมิน:\n{}\n\n{MENU}",
            user.current_level, evaluation.summary_feedback
        ),
    )
    .await
}

#[cfg(test)]
#[path = "line_webhook_tests.rs"]
mod tests;
