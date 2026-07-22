use super::dto::{RoleplayEvaluation, RoleplayReply, RoleplayScenario};
use super::ports::{RoleplayAiPort, RoleplayUseCase};
use crate::domain::chat_state::RoleplayTurn;
use crate::domain::error::AppResult;
use crate::domain::user::User;

pub struct RoleplayService<A: RoleplayAiPort> {
    ai: A,
}

impl<A: RoleplayAiPort> RoleplayService<A> {
    pub fn new(ai: A) -> Self {
        Self { ai }
    }
}

impl<A: RoleplayAiPort> RoleplayUseCase for RoleplayService<A> {
    async fn start_new_session(&self, user: &User) -> AppResult<RoleplayScenario> {
        self.ai.generate_scenario(user.current_level).await
    }

    async fn handle_turn(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[RoleplayTurn],
        user_message: &str,
    ) -> AppResult<RoleplayReply> {
        self.ai
            .respond_in_character(scenario, chat_history, user_message)
            .await
    }

    async fn grade_session(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[RoleplayTurn],
    ) -> AppResult<RoleplayEvaluation> {
        self.ai.evaluate_session(scenario, chat_history).await
    }
}
