#[derive(Debug, Clone, PartialEq)]
pub enum ChatState {
    Idle,
    VocabGuess,
    SentenceDraft,
    RoleplayActive,
}