use crate::application::sentence::dto::SentenceAnalysisResult;
use crate::application::sentence::ports::SentenceAiPort;
use super::client::GeminiClient;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct GeminiSentenceResponse {
    is_passed: bool,
    feedback: String,
}

impl SentenceAiPort for GeminiClient {
    async fn analyze_sentence(&self, current_text: &str) -> Result<SentenceAnalysisResult, String> {
        let prompt = format!(
            r#"Analyze the following English sentence drafted by the user: "{}"

            CRITICAL RULES for evaluation:
            1. If the sentence has grammatical errors, spelling mistakes, unnatural word choice, or incorrect tenses:
               - Set "is_passed": false
               - For "feedback": Explain the mistake and provide a hint/tip in simple Thai. **DO NOT provide the correct sentence or direct answer.** Let the user figure out how to fix it themselves.

            2. If the sentence is grammatically correct and sounds natural to a native speaker:
               - Set "is_passed": true
               - For "feedback": Give a brief praise in Thai and provide "Native Tricks" or alternative ways a native speaker would say this sentence to sound more professional (you can fully give examples and corrected forms in this case).

            Respond strictly as a JSON object: {{ "is_passed": bool, "feedback": string }}"#,
            current_text
        );

        let parsed: GeminiSentenceResponse = self
            .generate_json(
                Some("You are an expert native English coach specialized in sentence structures."),
                &prompt,
            )
            .await?;

        Ok(SentenceAnalysisResult {
            is_passed: parsed.is_passed,
            feedback: parsed.feedback,
        })
    }
}