use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::Value;

use crate::domain::chat_state::ChatState;
use crate::domain::user::User;
use crate::domain::vocab::VocabCategory;

use crate::application::user::ports::UserRepository;
use crate::application::vocab::ports::VocabRepository;

use crate::infrastructure::app_state::AppState;
use crate::application::user::ports::ChatStateRepository;
use crate::infrastructure::external::line_api::LineMessagingPort;

pub async fn handle_webhook(
    State(state): State<AppState>,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    if let Some(events) = payload.get("events").and_then(|e| e.as_array()) {
        for event in events {
            if event["type"] == "message" && event["message"]["type"] == "text" {
                let user_id = event["source"]["userId"].as_str().unwrap_or_default();
                let reply_token = event["replyToken"].as_str().unwrap_or_default();
                let user_text = event["message"]["text"].as_str().unwrap_or_default().trim();

                if !user_id.is_empty() && !reply_token.is_empty() {
                    if let Err(err) = process_user_message(&state, user_id, reply_token, user_text).await {
                        eprintln!("❌ Error processing message for {}: {}", user_id, err);
                        let _ = state.line_client.reply_text(
                            reply_token, 
                            "ขออภัยครับ ระบบเกิดข้อผิดพลาดในการประมวลผล กรุณาลองใหม่อีกครั้งครับ! 🛠️"
                        ).await;
                    }
                }
            }
        }
    }
    StatusCode::OK
}

async fn process_user_message(
    state: &AppState,
    user_id: &str,
    reply_token: &str,
    text: &str,
) -> Result<(), String> {
    let mut user = match state.user_repo.find_by_id(user_id).await? {
        Some(u) => u,
        None => {
            let new_user = User::new(user_id.to_string());
            state.user_repo.save(&new_user).await?;
            new_user
        }
    };

    let current_state = state.chat_state_repo.get_state(user_id).await?;

    if text == "ยกเลิก" || text == "ออก" || text == "exit" {
        state.chat_state_repo.clear_state(user_id).await?;
        return state.line_client.reply_text(
            reply_token, 
            "ออกสู่เมนูหลักแล้วครับ พิมพ์เลือกโหมดที่ต้องการฝึก:\n1. ทายศัพท์ (Vocab)\n2. ทบทวนศัพท์ (Review)\n3. แต่งประโยค (Sentence)\n4. โรลเพลย์ (Roleplay)"
        ).await;
    }

    match current_state {
        ChatState::Idle => handle_idle_state(state, &user, reply_token, text).await,
        ChatState::VocabGuessing { target_vocab_id, attempt } => {
            handle_vocab_guessing(state, &user, reply_token, text, target_vocab_id, attempt).await
        },
        ChatState::VocabReviewing { review_list, current_index } => {
            handle_vocab_reviewing(state, &user, reply_token, text, review_list, current_index).await
        },
        ChatState::SentenceDraft { sentence_id, fix_count } => {
            handle_sentence_draft(state, &user, reply_token, text, sentence_id, fix_count).await
        },
        ChatState::Roleplay { level, turn_count } => {
            handle_roleplay_turn(state, &mut user, reply_token, text, level, turn_count).await
        },
    }
}

async fn handle_idle_state(
    state: &AppState,
    user: &User,
    reply_token: &str,
    text: &str,
) -> Result<(), String> {
    match text {
        "1" | "ทายศัพท์" | "vocab" => {
            let vocabs = state.vocab_service.start_new_round(VocabCategory::Tech).await?; 
            state.vocab_service.save_completed_round(&user.user_id, vocabs.clone()).await?;

            if let Some(first_vocab) = vocabs.first() {
                state.chat_state_repo.set_state(
                    &user.user_id, 
                    &ChatState::VocabGuessing { target_vocab_id: first_vocab.vocab_id.clone(), attempt: 1 }, 
                    3600
                ).await?;

                let msg = format!(
                    "🔥 โหมดทายคำศัพท์เริ่มแล้ว!\n\n💡 คำแปล: \"{}\"\n📂 หมวดหมู่: {:?}\n\n👉 พิมพ์คำศัพท์ภาษาอังกฤษส่งมาได้เลยครับ!",
                    first_vocab.definition, first_vocab.category
                );
                state.line_client.reply_text(reply_token, &msg).await?;
            }
        },
        "2" | "ทบทวนศัพท์" | "review" => {
            let review_data = state.vocab_service.get_review_vocabs(&user.user_id).await?;
            let review_list: Vec<String> = review_data.into_iter().map(|(v, _)| v.vocab_id).collect();

            if let Some(first_vocab_id) = review_list.first() {
                if let Some(vocab) = state.vocab_repo.find_vocab_by_id(first_vocab_id).await? {
                    state.chat_state_repo.set_state(
                        &user.user_id,
                        &ChatState::VocabReviewing { review_list, current_index: 0 },
                        3600
                    ).await?;

                    let msg = format!(
                        "🔄 โหมดทบทวนศัพท์เก่า (ข้อที่ 1)\n\n💡 คำแปล: \"{}\"\n📂 หมวดหมู่: {:?}\n\n👉 พิมพ์คำศัพท์ภาษาอังกฤษที่คุณจำได้ส่งมาเลยครับ!",
                        vocab.definition, vocab.category
                    );
                    return state.line_client.reply_text(reply_token, &msg).await;
                }
            }
            state.line_client.reply_text(reply_token, "ยังไม่มีคำศัพท์ให้ทบทวนครับ ไปเล่นโหมดปกติก่อนนะ!").await?;
        },
        "3" | "แต่งประโยค" | "sentence" => {
            state.chat_state_repo.set_state(
                &user.user_id, 
                &ChatState::SentenceDraft { sentence_id: None, fix_count: 0 }, 
                3600
            ).await?;
            let msg = "✍️ โหมดฝึกแต่งประโยค\n\nพิมพ์ประโยคภาษาอังกฤษอะไรก็ได้ส่งมาเลยครับ AI จะช่วยตรวจและแนะทริคให้โดยไม่เฉลยคำตอบตรงๆ!";
            state.line_client.reply_text(reply_token, msg).await?;
        },
        "4" | "โรลเพลย์" | "roleplay" => {
            let scenario = state.roleplay_service.start_new_session(user).await?;
            state.chat_state_repo.set_state(
                &user.user_id, 
                &ChatState::Roleplay { level: user.current_level, turn_count: 1 }, 
                3600
            ).await?;

            let msg = format!(
                "🎭 โหมดสวมบทบาท (Level {})\n📌 สถานการณ์: {}\n🤖 บทบาท AI: {}\n\n💬 AI เริ่มคุย:\n\"{}\"\n\n👉 พิมพ์ตอบกลับอังกฤษเพื่อเริ่มสนุกได้เลยครับ!",
                user.current_level, scenario.setting, scenario.role_name, scenario.opening_line
            );
            state.line_client.reply_text(reply_token, &msg).await?;
        },
        _ => {
            let menu = "ยินดีต้อนรับสู่ EngOS! 🚀 ระบบอัปสกิลภาษาอังกฤษโปรแกรมเมอร์\n\nพิมพ์ตัวเลขเพื่อเลือกโหมดฝึก:\n1. ทายศัพท์ (Vocab)\n2. ทบทวนศัพท์ (Review)\n3. แต่งประโยค (Sentence)\n4. โรลเพลย์ (Roleplay)";
            state.line_client.reply_text(reply_token, menu).await?;
        }
    }
    Ok(())
}

async fn handle_vocab_guessing(
    state: &AppState,
    user: &User,
    reply_token: &str,
    text: &str,
    target_vocab_id: String,
    attempt: u8,
) -> Result<(), String> {
    let vocab = state.vocab_repo.find_vocab_by_id(&target_vocab_id).await?
        .ok_or_else(|| "Vocab not found".to_string())?;

    let eval = state.vocab_service.check_answer(&vocab, text).await?;

    if eval.is_correct {
        state.chat_state_repo.clear_state(&user.user_id).await?;
        let success_msg = format!("✅ ถูกต้องยอดเยี่ยมครับ!\n🎯 คำศัพท์คือ: \"{}\"\n⭐ Feedback: {}\n\n(กลับสู่เมนูหลักเรียบร้อย เลือกโหมดใหม่ได้เลยครับ)", vocab.word, eval.feedback);
        state.line_client.reply_text(reply_token, &success_msg).await?;
    } else {
        state.chat_state_repo.set_state(
            &user.user_id,
            &ChatState::VocabGuessing { target_vocab_id, attempt: attempt + 1 },
            3600
        ).await?;
        let fail_msg = format!("❌ ยังไม่ใช่ครับ! (ทายไปแล้ว {} ครั้ง)\n💡 คำใบ้จาก AI: {}\n\n👉 ลองเดาใหม่อีกครั้งส่งมาได้เลยครับ!", attempt, eval.feedback);
        state.line_client.reply_text(reply_token, &fail_msg).await?;
    }
    Ok(())
}

async fn handle_vocab_reviewing(
    state: &AppState,
    user: &User,
    reply_token: &str,
    text: &str,
    review_list: Vec<String>,
    current_index: usize,
) -> Result<(), String> {
    let current_vocab_id = &review_list[current_index];
    let vocab = state.vocab_repo.find_vocab_by_id(current_vocab_id).await?
        .ok_or_else(|| "Vocab not found".to_string())?;

    let eval = state.vocab_service.check_answer(&vocab, text).await?;

    let feedback_msg = if eval.is_correct {
        format!("✅ ถูกต้องครับ! คำศัพท์คือ \"{}\"\n⭐ Feedback: {}", vocab.word, eval.feedback)
    } else {
        format!("❌ ยังไม่ถูกครับ จริงๆ คือคำว่า \"{}\"\n⭐ Feedback: {}", vocab.word, eval.feedback)
    };

    let next_index = current_index + 1;
    if next_index < review_list.len() {
        let next_vocab_id = &review_list[next_index];
        let next_vocab = state.vocab_repo.find_vocab_by_id(next_vocab_id).await?
            .ok_or_else(|| "Next vocab not found".to_string())?;

        state.chat_state_repo.set_state(
            &user.user_id,
            &ChatState::VocabReviewing { review_list: review_list.clone(), current_index: next_index },
            3600
        ).await?;

        let next_msg = format!(
            "{}\n\n------------------\n🔄 คำศัพท์คำต่อไป (ข้อที่ {}/{})\n💡 คำแปล: \"{}\"\n📂 หมวดหมู่: {:?}\n\n👉 พิมพ์คำทายส่งมาเลยครับ!",
            feedback_msg, next_index + 1, review_list.len(), next_vocab.definition, next_vocab.category
        );
        state.line_client.reply_text(reply_token, &next_msg).await?;
    } else {
        state.chat_state_repo.clear_state(&user.user_id).await?;
        let final_msg = format!("{}\n\n🎉 ทบทวนคำศัพท์เก่าครบถ้วนทุกข้อแล้วครับ! เก่งมาก มุ่งสู่เมนูหลักกันต่อเลย!", feedback_msg);
        state.line_client.reply_text(reply_token, &final_msg).await?;
    }
    Ok(())
}

async fn handle_sentence_draft(
    state: &AppState,
    user: &User,
    reply_token: &str,
    text: &str,
    sentence_id: Option<String>,
    fix_count: u8,
) -> Result<(), String> {
    let s_id = sentence_id.unwrap_or_else(|| uuid::Uuid::new_v4().to_string());
    let mut sentence = state.sentence_service.start_new_draft(&s_id, &user.user_id, text);
    
    let analysis = state.sentence_service.evaluate_sentence(&mut sentence, text).await?;

    if analysis.is_passed {
        state.chat_state_repo.clear_state(&user.user_id).await?;
        let msg = format!("✅ ยอดเยี่ยมมากครับ! ประโยคสอบผ่านโครงสร้างเรียบรวย!\n\n💡 Native Trick สำหรับคุณ:\n{}\n\n(กลับสู่เมนูหลักเรียบร้อย พิมพ์เลือกโหมดใหม่ได้เลยครับ)", analysis.feedback);
        state.line_client.reply_text(reply_token, &msg).await?;
    } else {
        state.chat_state_repo.set_state(
            &user.user_id,
            &ChatState::SentenceDraft { sentence_id: Some(s_id), fix_count: fix_count + 1 },
            3600
        ).await?;
        let msg = format!("🧐 โครงสร้างยังไม่เป๊ะครับ! (แก้ไขไปแล้ว {} ครั้ง)\n\n💡 คำใบ้จาก AI Coach:\n{}\n\n👉 ลองปรับเปลี่ยนประโยคแล้วส่งมาใหม่อีกครั้งครับ!", fix_count + 1, analysis.feedback);
        state.line_client.reply_text(reply_token, &msg).await?;
    }
    Ok(())
}

async fn handle_roleplay_turn(
    state: &AppState,
    user: &mut User,
    reply_token: &str,
    text: &str,
    level: u8,
    turn_count: u8,
) -> Result<(), String> {
    let current_scenario = state.roleplay_service.start_new_session(user).await?; 
    let chat_history = vec![];

    if turn_count < 5 { 
        let reply = state.roleplay_service.handle_turn(&current_scenario, &chat_history, text).await?;
        let next_turn = turn_count + 1;
        
        state.chat_state_repo.set_state(
            &user.user_id, 
            &ChatState::Roleplay { level, turn_count: next_turn }, 
            3600
        ).await?;

        let msg = format!(
            "💬 [Turn {}/5] AI Roleplay:\n\"{}\"\n\n💡 คำใบ้ช่วยใบ้ตอบเทิร์นถัดไป: {}\n\n👉 พิมพ์ตอบกลับอังกฤษส่งมาได้เลยครับ!",
            next_turn, reply.ai_message, reply.hint.as_deref().unwrap_or("-")
        );
        state.line_client.reply_text(reply_token, &msg).await?;
    } else {
        let (eval, is_leveled_up) = state.roleplay_service.finish_session(user, &current_scenario, &chat_history).await?;
        
        state.user_repo.save(user).await?;
        state.chat_state_repo.clear_state(&user.user_id).await?;

        let status_icon = if is_leveled_up { 
            "🎉 ยินดีด้วยครับ! คุณสะสม Stack ครบและ LEVEL UP สำเร็จ!".to_string()
        } else if eval.is_passed {
            format!("💪 สอบผ่านประจำรอบ! สะสมความคืบหน้าเพิ่ม (Stack ปัจจุบัน: {})", user.progress_stack)
        } else {
            format!("❌ รอบนี้ยังไม่ผ่านเกณฑ์ครับ โดนหักแต้มสะสม (Stack ปัจจุบัน: {})", user.progress_stack)
        };

        let msg = format!(
            "🏁 จบเซสชันโรลเพลย์ครบถ้วน!\n📌 {}\n📊 ระดับปัจจุบัน: Level {}\n\n📋 สรุปผลการประเมิน:\n{}\n\n(พิมพ์เลือกโหมดฝึกอื่นๆ เพื่อลุยต่อได้เลยครับ)", 
            status_icon, user.current_level, eval.summary_feedback
        );
        state.line_client.reply_text(reply_token, &msg).await?;
    }
    Ok(())
}