use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    Json,
};
use serde_json::Value;
use crate::infrastructure::app_state::AppState;
use crate::domain::chat_state::ChatState;
use crate::domain::user::User;
use crate::application::vocab::ports::{VocabAiPort, VocabRepository};
use crate::application::sentence::ports::{SentenceAiPort, SentenceRepository};
use crate::application::user::ports::UserRepository;
use crate::infrastructure::database::redis_repo::ChatStateRepository;
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
                            "ขออภัยครับ ตอนนี้ระบบ AI กำลังประมวลผลหนักมาก ลองพิมพ์ส่งมาใหม่อีกครั้งนะครับ! 🛠️"
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
        return state.line_client.reply_text(reply_token, "ออกสู่เมนูหลักเรียบร้อยแล้วครับ! พิมพ์เลือกโหมดที่ต้องการฝึกได้เลย:\n1. ทายศัพท์\n2. แต่งประโยค\n3. โรลเพลย์").await;
    }

    match current_state {
        ChatState::Idle => handle_idle_state(state, &mut user, reply_token, text).await,
        ChatState::VocabGuessing(round) => handle_vocab_guessing(state, &mut user, reply_token, text, round).await,
        ChatState::SentenceDraft => handle_sentence_draft(state, &user, reply_token, text).await,
        ChatState::Roleplay { level, turn_count } => handle_roleplay_turn(state, &mut user, reply_token, text, level, turn_count).await,
        _ => {
            state.chat_state_repo.clear_state(user_id).await?;
            state.line_client.reply_text(reply_token, "กลับสู่เมนูหลักครับ พิมพ์เมนูที่ต้องการฝึกได้เลย!").await
        }
    }
}

async fn handle_idle_state(
    state: &AppState,
    user: &mut User,
    reply_token: &str,
    text: &str,
) -> Result<(), String> {
    match text {
        "1" | "ทายศัพท์" | "vocab" => {
            let vocabs = state.gemini_client.generate_three_vocabs().await?;
            if let Some(first_vocab) = vocabs.first() {
                state.vocab_repo.save_vocab(first_vocab).await?;
                state.chat_state_repo.set_state(&user.user_id, &ChatState::VocabGuessing(1), 3600).await?;
                
                let msg = format!("🔥 โหมดทายคำศัพท์ข้อที่ 1/3!\n\nคำศัพท์: \"{}\"\nหมวดหมู่: {:?}\n\n👉 พิมพ์คำแปลภาษาไทยหรืออังกฤษที่คุณคิดว่าตรงที่สุดส่งมาได้เลยครับ!", first_vocab.word, first_vocab.category);
                state.line_client.reply_text(reply_token, &msg).await?;
            }
        },
        "2" | "แต่งประโยค" | "sentence" => {
            state.chat_state_repo.set_state(&user.user_id, &ChatState::SentenceDraft, 3600).await?;
            let msg = "✍️ โหมดฝึกแต่งประโยค (Sentence Mode)\n\nให้คุณพิมพ์ประโยคภาษาอังกฤษอะไรก็ได้ที่คุณอยากฝึก หรือประโยคที่คุณใช้ทำงานวันนี้ส่งมาได้เลยครับ เดี๋ยว AI จะช่วยตรวจโครงสร้างและใบ้ทริคระดับ Native ให้เอง!";
            state.line_client.reply_text(reply_token, msg).await?;
        },
        "3" | "โรลเพลย์" | "roleplay" => {
            let scenario = state.roleplay_service.start_new_session(user).await?;
            state.chat_state_repo.set_state(&user.user_id, &ChatState::Roleplay { level: user.current_level, turn_count: 1 }, 3600).await?;
            
            let msg = format!("🎭 โหมดสวมบทบาท (Level {})\n📌 สถานการณ์: {}\n\n🤖 คู่สนทนา: {}\n\n💬 AI เริ่มคุย:\n\"{}\"\n\n👉 พิมพ์ตอบกลับเป็นภาษาอังกฤษเพื่อเริ่มเทิร์นที่ 1 ได้เลยครับ! (พิมพ์ 'ยกเลิก' เพื่อออกจากโหมด)", user.current_level, scenario.setting, scenario.role_name, scenario.opening_line);
            state.line_client.reply_text(reply_token, &msg).await?;
        },
        _ => {
            let menu = "ยินดีต้อนรับสู่ EngOS! 🚀 ระบบปฏิบัติการอัปสกิลภาษาอังกฤษสำหรับโปรแกรมเมอร์\n\nพิมพ์ตัวเลขหรือชื่อโหมดเพื่อเริ่มฝึก:\n1. ทายศัพท์ (Vocab)\n2. แต่งประโยค (Sentence)\n3. โรลเพลย์ (Roleplay)";
            state.line_client.reply_text(reply_token, menu).await?;
        }
    }
    Ok(())
}

async fn handle_vocab_guessing(
    state: &AppState,
    user: &mut User,
    reply_token: &str,
    _text: &str,
    round: u8,
) -> Result<(), String> {
    if round < 3 {
        let next_round = round + 1;
        state.chat_state_repo.set_state(&user.user_id, &ChatState::VocabGuessing(next_round), 3600).await?;
        let msg = format!("🎉 ยอดเยี่ยม! ไปต่อข้อที่ {}/3 กันเลย\n\n(ระบบกำลังโหลดคำศัพท์ต่อไป...)", next_round);
        state.line_client.reply_text(reply_token, &msg).await?;
    } else {
        user.progress_stack += 1;
        state.user_repo.save(user).await?;
        state.chat_state_repo.clear_state(&user.user_id).await?;
        
        let msg = format!("🏆 เก่งมากครับ! จบเกมทายคำศัพท์รอบนี้แล้ว\n⭐ สะสมรอบการฝึกปัจจุบัน: {} รอบ\n\nพิมพ์เลือกเมนูใหม่ได้ตลอดเวลาครับ!", user.progress_stack);
        state.line_client.reply_text(reply_token, &msg).await?;
    }
    Ok(())
}

async fn handle_sentence_draft(
    state: &AppState,
    user: &User,
    reply_token: &str,
    text: &str,
) -> Result<(), String> {
    let analysis = state.gemini_client.analyze_sentence(text).await?;

    if analysis.is_passed {
        let sentence = crate::domain::sentence::Sentence {
            sentence_id: uuid::Uuid::new_v4().to_string(),
            user_id: user.user_id.clone(),
            original_text: text.to_string(),
            total_fix: 1,
            final_feedback: analysis.feedback.clone(),
            is_passed: true,
        };
        state.sentence_repo.save_sentence(&sentence).await?;
        state.chat_state_repo.clear_state(&user.user_id).await?;

        let msg = format!("✅ โครงสร้างประโยคยอดเยี่ยมมากครับ! สอบผ่าน!\n\n💡 Native Trick สำหรับคุณ:\n{}\n\n(กลับสู่เมนูหลัก พิมพ์เลือกโหมดใหม่ได้เลยครับ)", analysis.feedback);
        state.line_client.reply_text(reply_token, &msg).await?;
    } else {
        let msg = format!("🧐 ยังไม่เป๊ะครับ ลองปรับดูอีกนิดนะ!\n\n💡 คำใบ้จาก AI Coach:\n{}\n\n👉 ลองพิมพ์ประโยคที่แก้ไขแล้วส่งมาใหม่ได้เลยครับ!", analysis.feedback);
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
    if turn_count < 10 {
        let dummy_scenario = crate::application::roleplay::dto::RoleplayScenario {
            role_name: "AI Partner".to_string(),
            setting: "Workplace discussion".to_string(),
            opening_line: "Hello!".to_string(),
        };
        
        let reply = state.roleplay_service.handle_turn(&dummy_scenario, &[], text).await?;
        let next_turn = turn_count + 1;
        state.chat_state_repo.set_state(&user.user_id, &ChatState::Roleplay { level, turn_count: next_turn }, 3600).await?;

        let hint_text = reply.hint.as_deref().unwrap_or("-");
        let msg = format!(
            "💬 [Turn {}/10] AI:\n\"{}\"\n\n💡 ศัพท์/คำใบ้ช่วยรอด: {}\n\n👉 พิมพ์ตอบกลับเทิร์นต่อไปได้เลยครับ!", 
            next_turn, 
            reply.ai_message, 
            hint_text 
        );        state.line_client.reply_text(reply_token, &msg).await?;
    } else {
        let dummy_scenario = crate::application::roleplay::dto::RoleplayScenario {
            role_name: "AI Partner".to_string(),
            setting: "Workplace discussion".to_string(),
            opening_line: "Hello!".to_string(),
        };
        
        let eval = state.roleplay_service.finish_session(user, &dummy_scenario, &[]).await?;
        
        state.user_repo.save(user).await?;
        state.chat_state_repo.clear_state(&user.user_id).await?;

        let status_icon = if eval.is_passed { "🎉 สอบผ่าน! LEVEL UP!" } else { "💪 พยายามได้ดีมากครับ แต่ยังไม่ผ่านเกณฑ์เลเวลนี้" };
        let msg = format!("🏁 จบการสนทนาครบ 10 เทิร์น!\n{}\n📌 เลเวลปัจจุบันของคุณ: Level {}\n\n📋 สรุปผลประเมินจาก AI Coach:\n{}\n\n(กลับสู่เมนูหลัก พิมพ์ 1, 2 หรือ 3 เพื่อฝึกต่อได้เลยครับ)", status_icon, user.current_level, eval.summary_feedback);
        state.line_client.reply_text(reply_token, &msg).await?;
    }
    Ok(())
}