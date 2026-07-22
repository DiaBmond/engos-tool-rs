use std::future::Future;

use crate::domain::chat_state::ChatState;
use crate::domain::error::AppResult;

/// Proof that the caller currently owns a user's conversation lock.
///
/// The token is random per acquisition so that releasing can verify ownership;
/// otherwise a slow handler whose lock had already expired would delete the
/// lock a *different* handler has since acquired.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LockToken {
    pub user_id: String,
    pub token: String,
}

/// Ephemeral conversation state, keyed by LINE user id.
pub trait ChatStateRepository: Send + Sync {
    fn get_state(&self, user_id: &str) -> impl Future<Output = AppResult<ChatState>> + Send;

    fn set_state(
        &self,
        user_id: &str,
        state: &ChatState,
        ttl_seconds: u64,
    ) -> impl Future<Output = AppResult<()>> + Send;

    fn clear_state(&self, user_id: &str) -> impl Future<Output = AppResult<()>> + Send;

    /// Liveness probe for the health endpoint.
    fn ping(&self) -> impl Future<Output = AppResult<()>> + Send;
}

/// Serialises concurrent messages from one user and suppresses duplicate
/// webhook deliveries.
///
/// LINE retries a webhook whenever it does not receive a prompt `200`, and a
/// learner can easily send two messages before the first finishes. Both cases
/// used to corrupt state through a read-modify-write race on Redis.
pub trait SessionLockRepository: Send + Sync {
    /// Attempts to take the per-user lock. `Ok(None)` means another task holds
    /// it — the caller should back off rather than block.
    fn try_acquire_lock(
        &self,
        user_id: &str,
        ttl_seconds: u64,
    ) -> impl Future<Output = AppResult<Option<LockToken>>> + Send;

    /// Releases the lock only if this token still owns it.
    fn release_lock(&self, token: &LockToken) -> impl Future<Output = AppResult<()>> + Send;

    /// Registers a webhook event id. Returns `true` the first time an id is
    /// seen and `false` for every replay within the retention window.
    fn try_claim_event(
        &self,
        event_id: &str,
        ttl_seconds: u64,
    ) -> impl Future<Output = AppResult<bool>> + Send;
}
