#[derive(Debug, Clone)]
pub struct UserVocab {
    pub user_id: String,
    pub vocab_id: String,
    pub guess_count: u32,
}

impl UserVocab {
    pub fn new(user_id: String, vocab_id: String) -> Self {
        Self {
            user_id,
            vocab_id,
            guess_count: 1, 
        }
    }

    pub fn add_guess_count(&mut self) {
        self.guess_count += 1;
    }
}