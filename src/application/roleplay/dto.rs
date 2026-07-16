use serde::{Serialize, Deserialize};
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoleplayScenario {
    pub role_name: String,
    pub setting: String,
    pub opening_line: String,
}

#[derive(Debug, Clone)]
pub struct RoleplayReply {
    pub ai_message: String,
    pub is_understood: bool,
    pub hint: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RoleplayEvaluation {
    pub is_passed: bool,
    pub summary_feedback: String,
}