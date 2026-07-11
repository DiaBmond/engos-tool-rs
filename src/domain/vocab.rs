#[derive(Debug, Clone, PartialEq)]
pub enum VocabCategory {
    Daily,
    Native,
    Tech,
}

#[derive(Debug, Clone)]
pub struct Vocab {
    pub vocab_id: String,
    pub word: String,
    pub definition: String,
    pub category: VocabCategory,
}

impl Vocab {
    pub fn new(vocab_id: String, word: String, definition: String, category: VocabCategory) -> Self {
        Self {
            vocab_id,
            word,
            definition,
            category,
        }
    }
}