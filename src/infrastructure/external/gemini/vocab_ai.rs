use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;
use crate::application::vocab::ports::VocabAiPort;
use crate::domain::vocab::{Vocab, VocabCategory};
use super::client::GeminiClient;

#[derive(Debug, Deserialize)]
struct GeminiVocabResponse {
    word: String,
    definition: String,
    category: String,
}

impl VocabAiPort for GeminiClient {
    async fn generate_three_vocabs(&self) -> Result<Vec<Vocab>, String> {
        let prompt = r#"
        คุณคือครูสอนภาษาอังกฤษระดับ Advanced สำหรับโปรแกรมเมอร์และคนทำงานด้านไอที
        ช่วยสร้างคำศัพท์ภาษาอังกฤษที่น่าสนใจ 3 คำ โดยแบ่งตามหมวดหมู่ดังนี้อย่างละ 1 คำ:
        1. "Daily" - คำศัพท์ใช้ในชีวิตประจำวันหรือการทำงานทั่วไป
        2. "Native" - สำนวน (Idiom), Phrasal Verb หรือคำแสลงที่เจ้าของภาษาชอบใช้
        3. "Tech" - คำศัพท์เฉพาะทางด้าน Software Engineering, Programming หรือ Tech Industry

        ให้ส่งกลับมาเป็น Array ของ Object ในรูปแบบ JSON เท่านั้น โดยมี Key คือ:
        - word: คำศัพท์ภาษาอังกฤษ
        - definition: คำแปลและอธิบายการใช้งานสั้นๆ โดนใจ เป็นภาษาไทย
        - category: ระบุหมวดหมู่ ("Daily", "Native" หรือ "Tech" ตัวพิมพ์ใหญ่นำหน้าตามนี้เป๊ะๆ)
        "#;

        let payload = json!({
            "contents": [{
                "parts": [{"text": prompt}]
            }],
            "generationConfig": {
                "temperature": 0.8,
                "response_mime_type": "application/json"
            }
        });

        let response_json = self.call_api(&payload).await?;

        let raw_text = response_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| "ไม่พบข้อความตอบกลับจาก Gemini API".to_string())?;

        let parsed_vocabs: Vec<GeminiVocabResponse> = serde_json::from_str(raw_text)
            .map_err(|e| format!("ไม่สามารถ Parse JSON จาก AI ได้: {}", e))?;

        let mut vocabs = Vec::new();
        for item in parsed_vocabs {
            let category = match item.category.as_str() {
                "Daily" => VocabCategory::Daily,
                "Native" => VocabCategory::Native,
                "Tech" => VocabCategory::Tech,
                _ => VocabCategory::Daily,
            };

            let vocab = Vocab::new(
                Uuid::new_v4().to_string(),
                item.word,
                item.definition,
                category,
            );
            vocabs.push(vocab);
        }

        Ok(vocabs)
    }
}