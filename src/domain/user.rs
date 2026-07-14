use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct User {
    pub user_id: String,
    pub current_level: u8,
    pub progress_stack: u16,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            current_level: 1,
            progress_stack: 0,
            created_at: Utc::now(),
        }
    }

    pub fn fail_roleplay(&mut self) {
        if self.progress_stack > 0 {
            self.progress_stack -= 1;
        }
    }

    pub fn pass_roleplay(&mut self) -> bool {
        self.progress_stack += 1;

        if self.current_level < 4 && self.progress_stack >= 5 {
            self.current_level += 1;
            self.progress_stack = 0; 
            return true;
        }

        false
    }
}