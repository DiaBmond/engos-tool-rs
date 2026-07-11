#[derive(Debug, Clone)]
pub struct Sentence {
    pub sentence_id: String,
    pub user_id: String,
    pub original_text: String,
    pub total_fix: u8,
    pub final_feedback: String,
    pub is_passed: bool,
}

impl Sentence {
    pub fn new(sentence_id: String, user_id: String, original_text: String) -> Self {
        Self {
            sentence_id,
            user_id,
            original_text,
            total_fix: 0,
            final_feedback: String::new(),
            is_passed: false,
        }
    }

    pub fn add_fix_count(&mut self) {
        self.total_fix += 1;
    }

    pub fn mark_as_passed(&mut self, feedback: String) {
        self.is_passed = true;
        self.final_feedback = feedback;
    }
}