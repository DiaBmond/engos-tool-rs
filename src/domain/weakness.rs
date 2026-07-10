#[derive(Debug, Clone, PartialEq)]
pub enum WeaknessTopic {
    Grammar,
    Vocabulary,
    Preposition,
    Tense,
    Other(String),
}

#[derive(Debug, Clone)]
pub struct Weakness {
    pub weakness_id: String,
    pub user_id: String,
    pub topic: WeaknessTopic,
    pub description: String,
    pub resolved: bool,
}