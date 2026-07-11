use std::future::Future;
use super::dto::{RoleplayScenario, RoleplayReply, RoleplayEvaluation};

pub trait RoleplayAiPort: Send + Sync {
    fn generate_scenario(&self, level: u8) -> impl Future<Output = Result<RoleplayScenario, String>> + Send;

    fn respond_in_character(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
        user_message: &str,
    ) -> impl Future<Output = Result<RoleplayReply, String>> + Send;

    fn evaluate_session(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
    ) -> impl Future<Output = Result<RoleplayEvaluation, String>> + Send;
}
