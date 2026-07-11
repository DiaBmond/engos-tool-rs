use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct User {
    pub user_id: String,
    pub current_level: u8, //roleplay
    pub progress_stack: u8, //vocab
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            current_level: 1,
            progress_stack: 0,
            created_at: chrono::Utc::now(),
        }
    }

    pub fn decrease_progress(&mut self) {
        if self.progress_stack > 0 {
            self.progress_stack -= 1;
        }
    }

    pub fn add_progress(&mut self) -> bool {
        self.progress_stack += 1;
        
        if self.progress_stack >= 5 {
            self.level_up();
            return true;
        }
        false
    }

    fn level_up(&mut self) {
        if self.current_level < 4 {
            self.current_level += 1;
        }
        self.progress_stack = 0;
    }
}