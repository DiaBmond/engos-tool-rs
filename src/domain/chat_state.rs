#[derive(Debug, Clone, PartialEq)]
pub enum ChatState {
    Idle,
    VocabGuessing(u8),
    VocabReviewing(u8),
    SentenceDraft,
    Roleplay { level: u8, turn_count: u8 },
}

impl ChatState {
    pub fn default_state() -> Self {
        ChatState::Idle
    }

    pub fn is_idle(&self) -> bool {
        *self == ChatState::Idle 
    }
}