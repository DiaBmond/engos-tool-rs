//! State-machine tests driven by in-memory fakes.
//!
//! These exercise `process_user_message` — the conversation state machine —
//! with no database, cache, AI or network. Every defect found in review lived
//! in this code, and until the transport layer was made generic over
//! [`AppDeps`] none of it could be tested at all.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use serde_json::{Value, json};

use super::*;
use crate::application::roleplay::dto::{RoleplayEvaluation, RoleplayReply};
use crate::application::sentence::dto::SentenceAnalysisResult;
use crate::application::sentence::ports::DraftOutcome;
use crate::application::vocab::dto::VocabEvaluation;
use crate::domain::user_vocab::UserVocab;
use crate::domain::vocab::{Vocab, VocabCategory};

// ---------------------------------------------------------------------------
// Fakes
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeUsers {
    user: Mutex<Option<User>>,
    saves: Mutex<Vec<User>>,
    awards: Mutex<u32>,
    penalties: Mutex<u32>,
}

impl UserUseCase for FakeUsers {
    async fn get_or_create(&self, user_id: &str) -> AppResult<User> {
        let mut slot = self.user.lock().unwrap();
        Ok(slot
            .get_or_insert_with(|| User::new(user_id.to_string()))
            .clone())
    }

    async fn award_progress(&self, user: &mut User) -> AppResult<bool> {
        *self.awards.lock().unwrap() += 1;
        let levelled = user.award_progress();
        *self.user.lock().unwrap() = Some(user.clone());
        self.saves.lock().unwrap().push(user.clone());
        Ok(levelled)
    }

    async fn penalize(&self, user: &mut User) -> AppResult<()> {
        *self.penalties.lock().unwrap() += 1;
        user.penalize();
        *self.user.lock().unwrap() = Some(user.clone());
        self.saves.lock().unwrap().push(user.clone());
        Ok(())
    }

    async fn health_check(&self) -> AppResult<()> {
        Ok(())
    }
}

struct FakeVocab {
    round: Vec<Vocab>,
    review: Vec<(Vocab, UserVocab)>,
    correct: Mutex<bool>,
    recorded: Mutex<Vec<(String, bool)>>,
    round_started: Mutex<u32>,
}

impl Default for FakeVocab {
    fn default() -> Self {
        Self {
            round: (0..VOCAB_ROUND_SIZE)
                .map(|i| vocab(&format!("v{i}")))
                .collect(),
            review: Vec::new(),
            correct: Mutex::new(true),
            recorded: Mutex::new(Vec::new()),
            round_started: Mutex::new(0),
        }
    }
}

impl VocabUseCase for FakeVocab {
    async fn start_new_round(&self, _user_id: &str) -> AppResult<Vec<Vocab>> {
        *self.round_started.lock().unwrap() += 1;
        Ok(self.round.clone())
    }

    async fn get_vocab(&self, vocab_id: &str) -> AppResult<Vocab> {
        self.round
            .iter()
            .chain(self.review.iter().map(|(v, _)| v))
            .find(|v| v.vocab_id == vocab_id)
            .cloned()
            .ok_or_else(|| AppError::InvalidState(format!("vocab {vocab_id} no longer exists")))
    }

    async fn check_answer(&self, _target: &Vocab, _answer: &str) -> AppResult<VocabEvaluation> {
        Ok(VocabEvaluation {
            is_correct: *self.correct.lock().unwrap(),
            feedback: "feedback".to_string(),
        })
    }

    async fn record_answer(
        &self,
        _user_id: &str,
        vocab_id: &str,
        was_correct: bool,
    ) -> AppResult<()> {
        self.recorded
            .lock()
            .unwrap()
            .push((vocab_id.to_string(), was_correct));
        Ok(())
    }

    async fn get_review_vocabs(&self, _user_id: &str) -> AppResult<Vec<(Vocab, UserVocab)>> {
        Ok(self.review.clone())
    }
}

#[derive(Default)]
struct FakeSentences {
    passes: Mutex<bool>,
    submissions: Mutex<Vec<(String, Option<String>, u8)>>,
}

impl SentenceUseCase for FakeSentences {
    async fn submit_draft(
        &self,
        _sentence_id: &str,
        _user_id: &str,
        draft_text: &str,
        original_text: Option<&str>,
        fix_count: u8,
    ) -> AppResult<DraftOutcome> {
        self.submissions.lock().unwrap().push((
            draft_text.to_string(),
            original_text.map(str::to_string),
            fix_count,
        ));

        let is_passed = *self.passes.lock().unwrap();
        Ok(DraftOutcome {
            analysis: SentenceAnalysisResult {
                is_passed,
                feedback: "coach feedback".to_string(),
            },
            total_fix: if is_passed { fix_count } else { fix_count + 1 },
            original_text: original_text.unwrap_or(draft_text).to_string(),
        })
    }
}

struct FakeRoleplay {
    passes: Mutex<bool>,
    understood: Mutex<bool>,
    graded: Mutex<u32>,
}

impl Default for FakeRoleplay {
    fn default() -> Self {
        Self {
            passes: Mutex::new(true),
            understood: Mutex::new(true),
            graded: Mutex::new(0),
        }
    }
}

impl RoleplayUseCase for FakeRoleplay {
    async fn start_new_session(&self, _user: &User) -> AppResult<RoleplayScenario> {
        Ok(scenario())
    }

    async fn handle_turn(
        &self,
        _scenario: &RoleplayScenario,
        _history: &[RoleplayTurn],
        _user_message: &str,
    ) -> AppResult<RoleplayReply> {
        Ok(RoleplayReply {
            ai_message: "ai says hi".to_string(),
            is_understood: *self.understood.lock().unwrap(),
            hint: Some("hint".to_string()),
        })
    }

    async fn grade_session(
        &self,
        _scenario: &RoleplayScenario,
        _history: &[RoleplayTurn],
    ) -> AppResult<RoleplayEvaluation> {
        *self.graded.lock().unwrap() += 1;
        Ok(RoleplayEvaluation {
            is_passed: *self.passes.lock().unwrap(),
            summary_feedback: "summary".to_string(),
        })
    }
}

#[derive(Default)]
struct FakeSession {
    states: Mutex<HashMap<String, ChatState>>,
    held_locks: Mutex<HashSet<String>>,
    claimed_events: Mutex<HashSet<String>>,
}

impl ChatStateRepository for FakeSession {
    async fn get_state(&self, user_id: &str) -> AppResult<ChatState> {
        Ok(self
            .states
            .lock()
            .unwrap()
            .get(user_id)
            .cloned()
            .unwrap_or(ChatState::Idle))
    }

    async fn set_state(&self, user_id: &str, state: &ChatState, _ttl: u64) -> AppResult<()> {
        self.states
            .lock()
            .unwrap()
            .insert(user_id.to_string(), state.clone());
        Ok(())
    }

    async fn clear_state(&self, user_id: &str) -> AppResult<()> {
        self.states.lock().unwrap().remove(user_id);
        Ok(())
    }

    async fn ping(&self) -> AppResult<()> {
        Ok(())
    }
}

impl SessionLockRepository for FakeSession {
    async fn try_acquire_lock(&self, user_id: &str, _ttl: u64) -> AppResult<Option<LockToken>> {
        let inserted = self.held_locks.lock().unwrap().insert(user_id.to_string());
        Ok(inserted.then(|| LockToken {
            user_id: user_id.to_string(),
            token: "t".to_string(),
        }))
    }

    async fn release_lock(&self, token: &LockToken) -> AppResult<()> {
        self.held_locks.lock().unwrap().remove(&token.user_id);
        Ok(())
    }

    async fn try_claim_event(&self, event_id: &str, _ttl: u64) -> AppResult<bool> {
        Ok(self
            .claimed_events
            .lock()
            .unwrap()
            .insert(event_id.to_string()))
    }
}

#[derive(Default)]
struct FakeMessaging {
    sent: Mutex<Vec<String>>,
}

impl MessagingPort for FakeMessaging {
    async fn reply_text(&self, _reply_token: &str, text: &str) -> AppResult<()> {
        self.sent.lock().unwrap().push(text.to_string());
        Ok(())
    }

    async fn push_text(&self, _user_id: &str, text: &str) -> AppResult<()> {
        self.sent.lock().unwrap().push(text.to_string());
        Ok(())
    }
}

#[derive(Default)]
struct Fakes {
    users: FakeUsers,
    vocab: FakeVocab,
    sentences: FakeSentences,
    roleplay: FakeRoleplay,
    session: FakeSession,
    messaging: FakeMessaging,
}

#[derive(Clone, Default)]
struct TestDeps(Arc<Fakes>);

impl AppDeps for TestDeps {
    type Users = FakeUsers;
    type Vocab = FakeVocab;
    type Sentences = FakeSentences;
    type Roleplay = FakeRoleplay;
    type Session = FakeSession;
    type Messaging = FakeMessaging;

    fn users(&self) -> &Self::Users {
        &self.0.users
    }
    fn vocab(&self) -> &Self::Vocab {
        &self.0.vocab
    }
    fn sentences(&self) -> &Self::Sentences {
        &self.0.sentences
    }
    fn roleplay(&self) -> &Self::Roleplay {
        &self.0.roleplay
    }
    fn session(&self) -> &Self::Session {
        &self.0.session
    }
    fn messaging(&self) -> &Self::Messaging {
        &self.0.messaging
    }
    fn line_channel_secret(&self) -> &str {
        "test_secret"
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const USER: &str = "U_test";

fn vocab(id: &str) -> Vocab {
    Vocab::new(
        id.to_string(),
        format!("word_{id}"),
        format!("นิยาม_{id}"),
        VocabCategory::Tech,
    )
}

fn scenario() -> RoleplayScenario {
    RoleplayScenario {
        role_name: "John".to_string(),
        setting: "ร้านกาแฟ".to_string(),
        opening_line: "Hi!".to_string(),
    }
}

fn msg(text: &str) -> TextMessage {
    TextMessage {
        event_id: Some("e1".to_string()),
        user_id: USER.to_string(),
        reply_token: "rt".to_string(),
        text: text.to_string(),
    }
}

impl TestDeps {
    async fn send(&self, text: &str) -> AppResult<()> {
        process_user_message(self, &msg(text)).await
    }

    fn state(&self) -> ChatState {
        self.0
            .session
            .states
            .lock()
            .unwrap()
            .get(USER)
            .cloned()
            .unwrap_or(ChatState::Idle)
    }

    fn set_state(&self, state: ChatState) {
        self.0
            .session
            .states
            .lock()
            .unwrap()
            .insert(USER.to_string(), state);
    }

    fn last_message(&self) -> String {
        self.0
            .messaging
            .sent
            .lock()
            .unwrap()
            .last()
            .cloned()
            .expect("a message should have been sent")
    }
}

// ---------------------------------------------------------------------------
// Idle / mode selection
// ---------------------------------------------------------------------------

#[tokio::test]
async fn unknown_input_shows_the_menu_and_leaves_state_idle() {
    let deps = TestDeps::default();
    deps.send("สวัสดี").await.unwrap();

    assert!(deps.last_message().contains("EngOS"));
    assert_eq!(deps.state(), ChatState::Idle);
}

#[tokio::test]
async fn selecting_vocab_starts_a_round_and_shows_the_first_word() {
    let deps = TestDeps::default();
    deps.send("1").await.unwrap();

    match deps.state() {
        ChatState::VocabGuessing {
            vocab_ids,
            current_index,
            attempt,
        } => {
            assert_eq!(vocab_ids.len(), VOCAB_ROUND_SIZE);
            assert_eq!(current_index, 0);
            assert_eq!(attempt, 1);
        }
        other => panic!("expected VocabGuessing, got {other:?}"),
    }
    assert!(deps.last_message().contains("ข้อที่ 1/3"));
}

/// A learner with no history must get a friendly prompt, not a system error.
/// This path used to return `Err` from the service and surface as "ระบบขัดข้อง".
#[tokio::test]
async fn review_with_no_history_gives_guidance_not_an_error() {
    let deps = TestDeps::default();
    let result = deps.send("2").await;

    assert!(result.is_ok(), "empty review must not be an error");
    assert!(deps.last_message().contains("ยังไม่มีคำศัพท์ให้ทบทวน"));
    assert_eq!(deps.state(), ChatState::Idle, "state must not advance");
}

#[tokio::test]
async fn exit_command_clears_state_from_any_mode() {
    let deps = TestDeps::default();
    deps.set_state(ChatState::Roleplay {
        turn_count: 3,
        scenario: scenario(),
        history: vec![],
    });

    deps.send("ยกเลิก").await.unwrap();

    assert_eq!(deps.state(), ChatState::Idle);
    assert!(deps.last_message().contains("ออกสู่เมนูหลัก"));
}

// ---------------------------------------------------------------------------
// Vocab guessing
// ---------------------------------------------------------------------------

#[tokio::test]
async fn correct_guess_advances_to_the_next_word() {
    let deps = TestDeps::default();
    deps.send("1").await.unwrap();
    deps.send("word_v0").await.unwrap();

    match deps.state() {
        ChatState::VocabGuessing {
            current_index,
            attempt,
            ..
        } => {
            assert_eq!(current_index, 1, "should move to the second word");
            assert_eq!(attempt, 1, "attempt counter resets for a new word");
        }
        other => panic!("expected VocabGuessing, got {other:?}"),
    }
    assert!(deps.last_message().contains("ข้อที่ 2/3"));
}

#[tokio::test]
async fn wrong_guess_increments_attempt_without_advancing() {
    let deps = TestDeps::default();
    *deps.0.vocab.correct.lock().unwrap() = false;
    deps.send("1").await.unwrap();
    deps.send("nope").await.unwrap();

    match deps.state() {
        ChatState::VocabGuessing {
            current_index,
            attempt,
            ..
        } => {
            assert_eq!(current_index, 0, "must stay on the same word");
            assert_eq!(attempt, 2);
        }
        other => panic!("expected VocabGuessing, got {other:?}"),
    }
}

/// Without a ceiling a learner could guess forever, and every wrong guess costs
/// an AI call.
#[tokio::test]
async fn answer_is_revealed_after_the_attempt_limit() {
    let deps = TestDeps::default();
    *deps.0.vocab.correct.lock().unwrap() = false;
    deps.send("1").await.unwrap();

    for _ in 0..MAX_VOCAB_ATTEMPTS {
        deps.send("nope").await.unwrap();
    }

    assert!(
        deps.last_message().contains("เฉลย"),
        "expected the answer to be revealed: {}",
        deps.last_message()
    );
    match deps.state() {
        ChatState::VocabGuessing { current_index, .. } => {
            assert_eq!(current_index, 1, "must move on after the limit");
        }
        other => panic!("expected VocabGuessing, got {other:?}"),
    }
}

#[tokio::test]
async fn finishing_a_round_awards_progress_once_and_clears_state() {
    let deps = TestDeps::default();
    deps.send("1").await.unwrap();
    for i in 0..VOCAB_ROUND_SIZE {
        deps.send(&format!("word_v{i}")).await.unwrap();
    }

    assert_eq!(deps.state(), ChatState::Idle, "round should end");
    assert_eq!(
        *deps.0.users.awards.lock().unwrap(),
        1,
        "progress must be awarded exactly once per round"
    );
    assert!(deps.last_message().contains("🏆"));
}

#[tokio::test]
async fn every_graded_answer_is_recorded_for_spaced_repetition() {
    let deps = TestDeps::default();
    *deps.0.vocab.correct.lock().unwrap() = false;
    deps.send("1").await.unwrap();
    deps.send("nope").await.unwrap();

    let recorded = deps.0.vocab.recorded.lock().unwrap();
    assert_eq!(recorded.len(), 1);
    assert_eq!(recorded[0], ("v0".to_string(), false));
}

/// A state written by an older build must degrade to a clear error rather than
/// panicking on an out-of-range index.
#[tokio::test]
async fn stale_state_index_reports_invalid_state_instead_of_panicking() {
    let deps = TestDeps::default();
    deps.set_state(ChatState::VocabGuessing {
        vocab_ids: vec!["v0".to_string()],
        current_index: 7,
        attempt: 1,
    });

    let error = deps.send("anything").await.unwrap_err();
    assert!(matches!(error, AppError::InvalidState(_)), "got {error:?}");
}

// ---------------------------------------------------------------------------
// Vocab review
// ---------------------------------------------------------------------------

#[tokio::test]
async fn finishing_a_review_clears_state_without_awarding_progress() {
    let deps = TestDeps::default();
    deps.set_state(ChatState::VocabReviewing {
        review_list: vec!["v0".to_string()],
        current_index: 0,
    });

    deps.send("word_v0").await.unwrap();

    assert_eq!(deps.state(), ChatState::Idle);
    assert_eq!(
        *deps.0.users.awards.lock().unwrap(),
        0,
        "review is practice and does not grant progress"
    );
    assert_eq!(deps.0.vocab.recorded.lock().unwrap().len(), 1);
}

// ---------------------------------------------------------------------------
// Sentence drafting
// ---------------------------------------------------------------------------

/// The revision count and the first draft must both survive across turns —
/// rebuilding them per message is what made `total_fix` always persist as 0.
#[tokio::test]
async fn sentence_revisions_carry_the_first_draft_and_fix_count_forward() {
    let deps = TestDeps::default();
    *deps.0.sentences.passes.lock().unwrap() = false;

    deps.send("3").await.unwrap();
    deps.send("I has a pen").await.unwrap();

    match deps.state() {
        ChatState::SentenceDraft {
            ref original_text,
            fix_count,
            ref sentence_id,
        } => {
            assert_eq!(original_text.as_deref(), Some("I has a pen"));
            assert_eq!(fix_count, 1);
            assert!(sentence_id.is_some(), "id must persist across turns");
        }
        ref other => panic!("expected SentenceDraft, got {other:?}"),
    }

    deps.send("I haves a pen").await.unwrap();

    let submissions = deps.0.sentences.submissions.lock().unwrap();
    assert_eq!(submissions.len(), 2);
    assert_eq!(
        submissions[1],
        (
            "I haves a pen".to_string(),
            Some("I has a pen".to_string()),
            1
        ),
        "second submission must carry the original draft and prior fix count"
    );
}

#[tokio::test]
async fn passing_a_sentence_clears_state() {
    let deps = TestDeps::default();
    *deps.0.sentences.passes.lock().unwrap() = true;

    deps.send("3").await.unwrap();
    deps.send("I have a pen").await.unwrap();

    assert_eq!(deps.state(), ChatState::Idle);
    assert!(deps.last_message().contains("ยอดเยี่ยม"));
}

// ---------------------------------------------------------------------------
// Roleplay
// ---------------------------------------------------------------------------

#[tokio::test]
async fn roleplay_turn_appends_history_and_advances_the_counter() {
    let deps = TestDeps::default();
    deps.send("4").await.unwrap();
    deps.send("Hello there").await.unwrap();

    match deps.state() {
        ChatState::Roleplay {
            turn_count,
            ref history,
            ..
        } => {
            assert_eq!(turn_count, 2);
            assert_eq!(history.len(), 1);
            assert_eq!(history[0].user_message, "Hello there");
            assert_eq!(history[0].ai_message, "ai says hi");
        }
        ref other => panic!("expected Roleplay, got {other:?}"),
    }
}

#[tokio::test]
async fn roleplay_warns_when_the_learner_is_not_understood() {
    let deps = TestDeps::default();
    *deps.0.roleplay.understood.lock().unwrap() = false;
    deps.send("4").await.unwrap();
    deps.send("asdfgh").await.unwrap();

    assert!(
        deps.last_message().contains("ยังไม่ค่อยเข้าใจ"),
        "got {}",
        deps.last_message()
    );
}

#[tokio::test]
async fn passing_the_final_roleplay_turn_awards_progress_and_ends_the_session() {
    let deps = TestDeps::default();
    *deps.0.roleplay.passes.lock().unwrap() = true;
    deps.set_state(ChatState::Roleplay {
        turn_count: ROLEPLAY_TOTAL_TURNS,
        scenario: scenario(),
        history: vec![],
    });

    deps.send("final answer").await.unwrap();

    assert_eq!(*deps.0.roleplay.graded.lock().unwrap(), 1);
    assert_eq!(*deps.0.users.awards.lock().unwrap(), 1);
    assert_eq!(*deps.0.users.penalties.lock().unwrap(), 0);
    assert_eq!(deps.state(), ChatState::Idle);
}

#[tokio::test]
async fn failing_the_final_roleplay_turn_penalizes_instead_of_awarding() {
    let deps = TestDeps::default();
    *deps.0.roleplay.passes.lock().unwrap() = false;
    deps.set_state(ChatState::Roleplay {
        turn_count: ROLEPLAY_TOTAL_TURNS,
        scenario: scenario(),
        history: vec![],
    });

    deps.send("gibberish").await.unwrap();

    assert_eq!(*deps.0.users.awards.lock().unwrap(), 0);
    assert_eq!(*deps.0.users.penalties.lock().unwrap(), 1);
    assert_eq!(deps.state(), ChatState::Idle);
}

/// Progression must be persisted by the same path every mode uses; grading
/// alone must never be the thing that saves a user.
#[tokio::test]
async fn roleplay_progression_is_persisted_through_the_user_service() {
    let deps = TestDeps::default();
    deps.set_state(ChatState::Roleplay {
        turn_count: ROLEPLAY_TOTAL_TURNS,
        scenario: scenario(),
        history: vec![],
    });

    deps.send("final answer").await.unwrap();

    let saves = deps.0.users.saves.lock().unwrap();
    assert_eq!(saves.len(), 1, "the graded user must be saved exactly once");
    assert_eq!(saves[0].progress_stack, 1);
}

// ---------------------------------------------------------------------------
// Event parsing
// ---------------------------------------------------------------------------

fn text_event() -> Value {
    json!({
        "type": "message",
        "webhookEventId": "01H",
        "replyToken": "rt-1",
        "source": { "type": "user", "userId": "U123" },
        "message": { "type": "text", "text": "  hello  " }
    })
}

#[test]
fn parses_a_well_formed_text_event() {
    let parsed = TextMessage::from_event(&text_event()).expect("should parse");
    assert_eq!(parsed.user_id, "U123");
    assert_eq!(parsed.reply_token, "rt-1");
    assert_eq!(parsed.text, "hello", "text should be trimmed");
    assert_eq!(parsed.event_id.as_deref(), Some("01H"));
}

#[test]
fn ignores_non_message_events() {
    let mut event = text_event();
    event["type"] = json!("follow");
    assert!(TextMessage::from_event(&event).is_none());
}

#[test]
fn ignores_non_text_messages() {
    let mut event = text_event();
    event["message"]["type"] = json!("sticker");
    assert!(TextMessage::from_event(&event).is_none());
}

/// Group and room events carry no `userId`.
#[test]
fn ignores_events_without_a_user_id() {
    let mut event = text_event();
    event["source"] = json!({ "type": "group", "groupId": "G1" });
    assert!(TextMessage::from_event(&event).is_none());
}

#[test]
fn ignores_events_with_an_empty_reply_token() {
    let mut event = text_event();
    event["replyToken"] = json!("");
    assert!(TextMessage::from_event(&event).is_none());
}

#[test]
fn ignores_whitespace_only_messages() {
    let mut event = text_event();
    event["message"]["text"] = json!("   ");
    assert!(TextMessage::from_event(&event).is_none());
}

#[test]
fn tolerates_a_missing_event_id() {
    let mut event = text_event();
    event.as_object_mut().unwrap().remove("webhookEventId");
    let parsed = TextMessage::from_event(&event).expect("should still parse");
    assert!(parsed.event_id.is_none());
}

#[test]
fn recognises_all_exit_commands_case_insensitively() {
    for cmd in ["ยกเลิก", "ออก", "exit", "EXIT", " Cancel "] {
        assert!(is_exit_command(cmd), "{cmd} should exit");
    }
    assert!(!is_exit_command("1"));
    assert!(!is_exit_command("exit the building"));
}

// ---------------------------------------------------------------------------
// Deduplication and locking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn a_replayed_event_id_is_skipped() {
    let deps = TestDeps::default();
    let message = msg("hello");

    assert!(!is_duplicate(&deps, &message).await, "first delivery");
    assert!(is_duplicate(&deps, &message).await, "retry must be skipped");
}

#[tokio::test]
async fn a_contended_lock_is_retried_then_reported() {
    let deps = TestDeps::default();
    // Simulate another turn already holding the lock.
    deps.0
        .session
        .held_locks
        .lock()
        .unwrap()
        .insert(USER.to_string());

    let outcome = acquire_lock(&deps, USER).await.unwrap();
    assert!(outcome.is_none(), "must give up rather than block forever");
}

#[tokio::test]
async fn a_free_lock_is_acquired_and_released() {
    let deps = TestDeps::default();

    let lock = acquire_lock(&deps, USER).await.unwrap().expect("acquired");
    assert!(deps.0.session.held_locks.lock().unwrap().contains(USER));

    deps.0.session.release_lock(&lock).await.unwrap();
    assert!(!deps.0.session.held_locks.lock().unwrap().contains(USER));
}

/// The lock must outlive the longest turn, or two messages can interleave.
#[test]
fn turn_deadline_is_shorter_than_the_lock_ttl() {
    assert!(
        TURN_DEADLINE.as_secs() < LOCK_TTL_SECONDS,
        "a turn must finish before its lock expires"
    );
}
