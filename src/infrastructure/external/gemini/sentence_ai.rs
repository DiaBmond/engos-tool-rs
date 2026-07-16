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
            1. Focus ONLY on major grammatical errors, spelling mistakes, tense usage, and natural sentence structures.
            2. DO NOT be overly pedantic or strict about minor capitalization after commas (e.g., "Hi, My name" is acceptable) or minor punctuation unless it completely changes the meaning.
            3. If the sentence has MAJOR errors:
               - Set "is_passed": false
               - For "feedback": Explain the mistake and provide a hint/tip in simple Thai. **DO NOT provide the correct sentence or direct answer.**
            4. If the sentence is generally correct and understandable to a native speaker (even with minor informalities):
               - Set "is_passed": true
               - For "feedback": Give a brief praise in Thai and provide "Native Tricks" or alternative ways a native speaker would say this."#,
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