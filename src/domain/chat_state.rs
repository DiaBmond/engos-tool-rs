#[derive(Debug, Clone, PartialEq)]
pub enum ChatState {
    Idle,
    VocabGuess,
    SentenceDraft,
    RoleplayActive,
}

impl ChatState {
    pub fn default_state() -> Self {
        ChatState::Idle
    }

    pub fn is_idle(&self) -> bool {
        *self == ChatState::Idle 
    }
}