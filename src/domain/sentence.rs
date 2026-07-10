use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Sentence {
    pub sentence_id: String,
    pub user_id: String,
    pub total_fix: u8,
    pub ai_feedback: String,
    pub created_at: DateTime<Utc>,
}