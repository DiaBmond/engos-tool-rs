use std::future::Future;

use crate::domain::error::AppResult;
use crate::domain::user::User;

/// Driven port: user persistence.
pub trait UserRepository: Send + Sync {
    fn find_by_id(&self, user_id: &str) -> impl Future<Output = AppResult<Option<User>>> + Send;

    fn save(&self, user: &User) -> impl Future<Output = AppResult<()>> + Send;

    /// Liveness probe for the health endpoint.
    fn ping(&self) -> impl Future<Output = AppResult<()>> + Send;
}

/// Driving port: what the transport layer may do with learners.
///
/// `UserService` is the only implementation in production; tests substitute a
/// fake so the conversation state machine can be exercised without a database.
pub trait UserUseCase: Send + Sync {
    fn get_or_create(&self, user_id: &str) -> impl Future<Output = AppResult<User>> + Send;

    /// Grants progress for one completed session and persists the result.
    /// Returns `true` when the learner levelled up.
    ///
    /// This is the single entry point for progression — every mode goes through
    /// it so the level-up rule cannot drift between them.
    fn award_progress(&self, user: &mut User) -> impl Future<Output = AppResult<bool>> + Send;

    /// Applies the failure penalty and persists the result.
    fn penalize(&self, user: &mut User) -> impl Future<Output = AppResult<()>> + Send;

    fn health_check(&self) -> impl Future<Output = AppResult<()>> + Send;
}
