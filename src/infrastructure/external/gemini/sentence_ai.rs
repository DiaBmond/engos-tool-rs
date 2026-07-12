use serde::Deserialize;
use serde_json::json;
use crate::application::sentence::dto::SentenceAnalysisResult;
use crate::application::sentence::ports::SentenceAiPort;
use super::client::GeminiClient;

#[derive(Debug, Deserialize)]
struct GeminiSentenceResponse {
    is_passed: bool,
    feedback: String,
}

impl SentenceAiPort for GeminiClient {
    async fn analyze_sentence(&self, current_text: &str) -> Result<SentenceAnalysisResult, String> {
        let prompt = format!(
            r#"
            คุณคือโค้ชภาษาอังกฤษระดับ Native ผู้เชี่ยวชาญด้านการปรับโครงสร้างประโยค (Sentence Structure)
            โปรดวิเคราะห์ประโยคภาษาอังกฤษที่ผู้ใช้ออกแบบมา: "{}"

            ให้ประเมินและส่งกลับมาเป็น JSON ตามกฎเหล็กดังนี้เท่านั้น:
            1. ถ้าประโยคยังมีข้อผิดพลาดด้านไวยากรณ์ (Grammar), สื่อสารไม่รู้เรื่อง หรือใช้คำดูไม่เป็นธรรมชาติ (Tense ผิด, Word Order ผิด ฯลฯ):
               - ให้กำหนด "is_passed": false
               - ในช่อง "feedback": ให้บอกว่าผิดจุดไหน และ "ใบ้ทริคการแก้ไข" โดย **ห้ามพิมพ์ประโยคที่ถูกต้องสมบูรณ์ (ห้ามเฉลยตรงๆ) เด็ดขาด!** เพื่อให้ผู้ใช้คิดแก้ด้วยตัวเอง
            
            2. ถ้าประโยคถูกต้องตามไวยากรณ์และสละสลวยในระดับที่ชาวต่างชาติเข้าใจได้ธรรมชาติ:
               - ให้กำหนด "is_passed": true
               - ในช่อง "feedback": ให้ชื่นชมสั้นๆ และมอบ "Native Trick" หรือคำแนะนำเพิ่มเติมว่าชาวต่างชาติชอบพูดประโยคนี้ในรูปแปบไหนให้ดูโปรขึ้น (ในขั้นตอนนี้เฉลยและยกตัวอย่างประโยคได้เต็มที่)

            ตอบกลับเป็น JSON Object เท่านั้น โดยมี Key คือ "is_passed" (Boolean) และ "feedback" (String คำอธิบายภาษาไทยที่กระชับ โดนใจ)
            "#,
            current_text
        );

        let payload = json!({
            "contents": [{
                "parts": [{"text": prompt}]
            }],
            "generationConfig": {
                "temperature": 0.4,
                "response_mime_type": "application/json"
            }
        });

        let response_json = self.call_api(&payload).await?;

        let raw_text = response_json["candidates"][0]["content"]["parts"][0]["text"]
            .as_str()
            .ok_or_else(|| "ไม่พบข้อความตอบกลับจาก Gemini API".to_string())?;

        let parsed: GeminiSentenceResponse = serde_json::from_str(raw_text)
            .map_err(|e| format!("ไม่สามารถ Parse JSON จาก AI ได้: {}", e))?;

        Ok(SentenceAnalysisResult {
            is_passed: parsed.is_passed,
            feedback: parsed.feedback,
        })
    }
}