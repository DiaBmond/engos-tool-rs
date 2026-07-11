use super::dto::{RoleplayScenario, RoleplayReply, RoleplayEvaluation};
use super::ports::RoleplayAiPort;
use crate::domain::user::User;

pub struct RoleplayService<A: RoleplayAiPort> {
    ai: A,
}

impl<A: RoleplayAiPort> RoleplayService<A> {
    pub fn new(ai: A) -> Self {
        Self { ai }
    }

    pub async fn start_new_session(&self, user: &User) -> Result<RoleplayScenario, String> {
        self.ai.generate_scenario(user.current_level).await
    }

    pub async fn handle_turn(
        &self,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
        user_message: &str,
    ) -> Result<RoleplayReply, String> {
        self.ai.respond_in_character(scenario, chat_history, user_message).await
    }

    pub async fn finish_session(
        &self,
        user: &mut User,
        scenario: &RoleplayScenario,
        chat_history: &[(String, String)],
    ) -> Result<RoleplayEvaluation, String> {
        let eval = self.ai.evaluate_session(scenario, chat_history).await?;

        if eval.is_passed {
            user.level_up();
        }

        Ok(eval)
    }
}