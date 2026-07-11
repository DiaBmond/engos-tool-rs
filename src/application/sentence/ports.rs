use std::future::Future;
use crate::domain::sentence::Sentence;
use super::dto::SentenceAnalysisResult;

pub trait SentenceRepository: Send + Sync {
    fn save_sentence(&self, sentence: &Sentence) -> impl Future<Output = Result<(), String>> + Send;
}

pub trait SentenceAiPort: Send + Sync {
    fn analyze_sentence(&self, current_text: &str) -> impl Future<Output = Result<SentenceAnalysisResult, String>> + Send;
}