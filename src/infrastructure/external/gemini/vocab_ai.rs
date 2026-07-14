use crate::application::vocab::dto::VocabEvaluation;
use crate::application::vocab::ports::VocabAiPort;
use crate::domain::vocab::{Vocab, VocabCategory};
use super::client::GeminiClient;
use serde::Deserialize;
use uuid::Uuid;

#[derive(Deserialize)]
struct GeminiVocabResponse {
    word: String,
    definition: String,
    category: String,
}

impl VocabAiPort for GeminiClient {
    async fn generate_vocabs_by_category(&self, category: VocabCategory, count: usize) -> Result<Vec<Vocab>, String> {
        let cat_str = format!("{:?}", category);
        let prompt = format!(
            r#"Generate {} English vocabulary words belonging to the category: "{}".

            Definition of categories:
            - Daily: Common words used in everyday conversation.
            - Native: Natural expressions, idioms, or phrasal verbs used by native speakers.
            - Tech: Professional terms and jargon specific to software development and IT.

            Return the result as a JSON array of objects with the following fields: 
            "word" (string), "definition" (string), "category" (string).
            Ensure the "category" value matches one of the categories provided above exactly."#,
            count, cat_str
        );

        let raw_vocabs: Vec<GeminiVocabResponse> = self
            .generate_json(
                Some("You are an expert English teacher for developers."),
                &prompt,
            )
            .await?;

        Ok(raw_vocabs
            .into_iter()
            .map(|item| {
                let cat = match item.category.as_str() {
                    "Daily" => VocabCategory::Daily,
                    "Native" => VocabCategory::Native,
                    "Tech" => VocabCategory::Tech,
                    _ => VocabCategory::Daily,
                };
                Vocab::new(Uuid::new_v4().to_string(), item.word, item.definition, cat)
            })
            .collect())
    }

    async fn evaluate_vocab_guess(&self, vocab: &Vocab, user_guess: &str) -> Result<VocabEvaluation, String> {
        let prompt = format!(
            r#"Target Word: "{}"
            Target Definition: "{}"
            User's Input: "{}"

            Task: Evaluate the user's input.
            - The goal is for the user to recall the Target Word in English.
            - If the user's input correctly matches the meaning (even in another language or as a synonym), mark "is_correct": true.
            - If "is_correct": true but the input was not the exact English word, explain why it was correct but gently encourage the user to use the specific English term: "{}" in their future practice.
            - If incorrect, provide feedback in simple English explaining the correct meaning of "{}" and how to use it.
            - Respond strictly as a JSON object: {{ "is_correct": bool, "feedback": string }}"#,
            vocab.word, vocab.definition, user_guess, vocab.word, vocab.word
        );

        self.generate_json(
            Some("You are an expert English teacher for developers."),
            &prompt,
        )
        .await
    }
}