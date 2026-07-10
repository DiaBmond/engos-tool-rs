#[derive(Debug, Clone)]
pub struct Vocab {
    pub vocab_id: String,
    pub word: String,
    pub definition: String,
    pub category: String,
}

impl Vocab {
    pub fn new(vocab_id: String, word: String, definition: String, category: String) -> Self {
        Self {
            vocab_id,
            word,
            definition,
            category,
        }
    }
}