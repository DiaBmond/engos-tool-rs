use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VocabEvaluation {
    pub is_correct: bool,
    pub feedback: String,
}
