use std::future::Future;

use super::dto::{RoleplayEvaluation, RoleplayReply, RoleplayScenario};
use crate::domain::chat_state::RoleplayTurn;
use crate::domain::error::AppResult;
use crate::domain::user::User;

/// Driven port: roleplay direction, acting and grading.
pub trait RoleplayAiPort: Send + Sync {
    fn generate_scenario(
        &self,
        level: u8,
    ) -> impl Future<Output = AppResult<RoleplayScenario>> + Send;

    fn respond_in_character(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[RoleplayTurn],
        user_message: &str,
    ) -> impl Future<Output = AppResult<RoleplayReply>> + Send;

    fn evaluate_session(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[RoleplayTurn],
    ) -> impl Future<Output = AppResult<RoleplayEvaluation>> + Send;
}

/// Driving port: what the transport layer may do with roleplay sessions.
pub trait RoleplayUseCase: Send + Sync {
    fn start_new_session(
        &self,
        user: &User,
    ) -> impl Future<Output = AppResult<RoleplayScenario>> + Send;

    fn handle_turn(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[RoleplayTurn],
        user_message: &str,
    ) -> impl Future<Output = AppResult<RoleplayReply>> + Send;

    /// Grades a finished session.
    ///
    /// Deliberately free of side effects: it does **not** touch the learner.
    /// Applying the result goes through `UserUseCase` like every other mode, so
    /// progression has exactly one owner. The earlier version mutated the user
    /// here and left persistence to the caller, which made it easy to grade a
    /// session and then forget to save it.
    fn grade_session(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[RoleplayTurn],
    ) -> impl Future<Output = AppResult<RoleplayEvaluation>> + Send;
}
