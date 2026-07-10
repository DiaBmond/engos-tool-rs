use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct Sentence {
    pub sentence_id: String,
    pub user_id: String,
    pub total_fix: u8,
    pub ai_feedback: String,
    pub created_at: DateTime<Utc>,
}

impl Sentence{
    pub new(sentence_id: String, user_id:String) -> Self {
        Self {
            sentence_id,
            user_id,
            total_fix: 0,
            ai_feedback: String::new(),
        }
    }

    pub fn update_feedback(&mut self, new_feedback: String){
        self.ai_feedback = new_feedback;
        self.total_fix += 1;
    }
}