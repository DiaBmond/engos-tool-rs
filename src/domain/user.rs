use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct User {
    pub user_id: String,
    pub current_level: u8,
    pub progress_stack: u8,
    pub created_at: DateTime<Utc>,
}