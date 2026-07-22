use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayScenario {
    pub role_name: String,
    pub setting: String,
    pub opening_line: String,
}

#[derive(Debug, Clone)]
pub struct RoleplayReply {
    pub ai_message: String,
    /// Whether the learner's message made sense in context. Drives an extra
    /// nudge in the reply rather than being silently discarded.
    pub is_understood: bool,
    pub hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RoleplayEvaluation {
    pub is_passed: bool,
    pub summary_feedback: String,
}
