use serde::Deserialize;
use serde_json::json;
use crate::application::roleplay::dto::{RoleplayScenario, RoleplayReply, RoleplayEvaluation};
use crate::application::roleplay::ports::RoleplayAiPort;
use super::client::GeminiClient;

#[derive(Debug, Deserialize)]
struct GeminiScenarioResponse {
    role_name: String,
    setting: String,
    opening_line: String,
}

#[derive(Debug, Deserialize)]
struct GeminiReplyResponse {
    ai_message: String,
    is_understood: bool,
    hint: String,
}

#[derive(Debug, Deserialize)]
struct GeminiEvalResponse {
    is_passed: bool,
    summary_feedback: String,
}

impl RoleplayAiPort for GeminiClient {
    
    async fn generate_scenario(&self, level: u8) -> Result<RoleplayScenario, String> {
        let difficulty_desc = match level {
            1 => "Level 1 (Beginner): สถานการณ์ในชีวิตประจำวันง่ายๆ เช่น สั่งอาหาร, ถามทาง, ซื้อของ (ใช้คำศัพท์พื้นฐาน ประโยคสั้นๆ)",
            2 => "Level 2 (Intermediate): สถานการณ์ในที่ทำงาน หรือการท่องเที่ยวที่ต้องแก้ปัญหา เช่น เช็คอินโรงแรมที่ห้องเต็ม, คุยงานกับเพื่อนร่วมงานต่างชาติ",
            3 => "Level 3 (Advanced): สถานการณ์ที่ต้องใช้ความมั่นใจและการอธิบาย เช่น สัมภาษณ์งานบริษัท Tech, เจรจาต่อรองกับลูกค้า, พรีเซนต์โปรเจกต์",
            4 => "Level 4 (Native/Master): สถานการณ์รับมือกับวิกฤตความกดดันสูง เช่น เคลียร์ปัญหา Production Outage กับผู้บริหาร, เจรจาไกล่เกลี่ยข้อพิพาทเชิงธุรกิจ",
            _ => "Level 1 (Beginner): สถานการณ์ทั่วไปในชีวิตประจำวัน",
        };

        let prompt = format!(
            r#"
            คุณคือผู้กำกับเกมจำลองสถานการณ์ภาษาอังกฤษ (English Roleplay AI)
            ช่วยสร้างสถานการณ์จำลองสำหรับการฝึกคุยภาษาอังกฤษ 1 สถานการณ์ โดยมีความยากระดับ:
            "{}"

            ให้ตอบกลับมาเป็น JSON Object เท่านั้น โดยมี Key ดังนี้:
            - role_name: ชื่อและบทบาทที่ AI จะต้องแสดง (เช่น "John, an angry senior developer")
            - setting: คำอธิบายบริบทของสถานการณ์ และกำหนดชัดเจนว่า "ผู้ใช้" ต้องรับบทเป็นใครและต้องทำอะไรให้สำเร็จ (อธิบายเป็นภาษาไทย)
            - opening_line: ประโยคเปิดบทสนทนาประโยคแรกจาก AI คุยกับผู้ใช้เป็นภาษาอังกฤษให้ตรงตามบทบาท
            "#,
            difficulty_desc
        );

        let payload = json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": { "temperature": 0.8, "response_mime_type": "application/json" }
        });

        let res = self.call_api(&payload).await?;
        let raw_text = res["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or_default();
        let parsed: GeminiScenarioResponse = serde_json::from_str(raw_text)
            .map_err(|e| format!("Parse Scenario JSON error: {}", e))?;

        Ok(RoleplayScenario {
            role_name: parsed.role_name,
            setting: parsed.setting,
            opening_line: parsed.opening_line,
        })
    }

    async fn respond_in_character(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
        user_message: &str,
    ) -> Result<RoleplayReply, String> {
        let mut history_text = String::new();
        for (u_msg, ai_msg) in chat_history {
            history_text.push_str(&format!("User: {}\nAI ({}): {}\n", u_msg, scenario.role_name, ai_msg));
        }

        let prompt = format!(
            r#"
            คุณกำลังสวมบทบาทเป็น "{}" ในสถานการณ์: "{}"
            นี่คือประวัติการสนทนาที่ผ่านมา:
            {}
            ผู้ใช้เพิ่งพูดว่า: "{}"

            กรุณาตอบกลับในบทบาทของคุณอย่างสมจริงและเป็นธรรมชาติในฐานะเจ้าของภาษา โดยตอบเป็น JSON Object เท่านั้น มี Key คือ:
            - ai_message: ประโยคตอบกลับของคุณเป็นภาษาอังกฤษ (ห้ามหลุดบทบาทเด็ดขาด)
            - is_understood: Boolean (true ถ้าผู้ใช้พิมพ์สื่อสารภาษาอังกฤษมาเข้าใจความหมายได้, false ถ้าพิมพ์ผิดบริบทหรืออ่านไม่รู้เรื่องจนคุณตอบไม่ถูก)
            - hint: คำแนะนำหรือคำศัพท์ที่เป็นประโยชน์ภาษาไทยสั้นๆ เพื่อ "ช่วยใบ้" ให้ผู้ใช้รู้ว่าเทิร์นถัดไปควรตอบประมาณไหน หรือใช้โครงสร้างประโยคอะไรดี
            "#,
            scenario.role_name, scenario.setting, history_text, user_message
        );

        let payload = json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": { "temperature": 0.7, "response_mime_type": "application/json" }
        });

        let res = self.call_api(&payload).await?;
        let raw_text = res["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or_default();
        let parsed: GeminiReplyResponse = serde_json::from_str(raw_text)
            .map_err(|e| format!("Parse Reply JSON error: {}", e))?;

        Ok(RoleplayReply {
            ai_message: parsed.ai_message,
            is_understood: parsed.is_understood,
            hint: Some(parsed.hint), // ใส่ Some() ครอบเพราะใน DTO เป็น Option<String>
        })
    }

    async fn evaluate_session(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
    ) -> Result<RoleplayEvaluation, String> {
        let mut history_text = String::new();
        for (u_msg, ai_msg) in chat_history {
            history_text.push_str(&format!("User: {}\nAI: {}\n", u_msg, ai_msg));
        }

        let prompt = format!(
            r#"
            คุณคือกรรมการประเมินความสามารถด้านการสื่อสารภาษาอังกฤษ
            โปรดพิจารณาบทสนทนาจำลองสถานการณ์ "{}" ทั้งหมดนี้:
            {}

            จงประเมินว่าผู้ใช้สื่อสารภาษาอังกฤษได้มีประสิทธิภาพ สำเร็จเป้าหมายของสถานการณ์นี้ และใช้ไวยากรณ์ได้เหมาะสมหรือไม่:
            - ถ้าสื่อสารได้ดี เข้าใจง่าย เอาตัวรอดได้สำเร็จ (แม้จะมีผิดแกรมม่าเล็กๆ น้อยๆ แต่ไม่เสียใจความ): ให้กำหนด "is_passed": true
            - ถ้าสื่อสารไม่รู้เรื่อง ใช้คำศัพท์ผิดบริบทอย่างรุนแรง หรือตอบไม่ตรงคำถามตลอดเวลา: ให้กำหนด "is_passed": false

            ตอบกลับเป็น JSON Object เท่านั้น มี Key คือ:
            - is_passed: Boolean (true ถ้าผ่าน, false ถ้าไม่ผ่าน)
            - summary_feedback: คำวิจารณ์เชิงสร้างสรรค์เป็นภาษาไทย สรุปข้อดี และจุดที่ควรปรับปรุงเพื่ออัปเลเวลในครั้งถัดไป
            "#,
            scenario.setting, history_text
        );

        let payload = json!({
            "contents": [{ "parts": [{ "text": prompt }] }],
            "generationConfig": { "temperature": 0.3, "response_mime_type": "application/json" }
        });

        let res = self.call_api(&payload).await?;
        let raw_text = res["candidates"][0]["content"]["parts"][0]["text"].as_str().unwrap_or_default();
        let parsed: GeminiEvalResponse = serde_json::from_str(raw_text)
            .map_err(|e| format!("Parse Eval JSON error: {}", e))?;

        Ok(RoleplayEvaluation {
            is_passed: parsed.is_passed,
            summary_feedback: parsed.summary_feedback,
        })
    }
}