use redis::aio::ConnectionManager;
use redis::{AsyncCommands, ExistenceCheck, SetExpiry, SetOptions};
use uuid::Uuid;

use crate::application::session::ports::{ChatStateRepository, LockToken, SessionLockRepository};
use crate::domain::chat_state::ChatState;
use crate::domain::error::{AppError, AppResult};

/// Bumped whenever [`ChatState`]'s serialized shape changes, so a deploy cannot
/// hand old payloads to a new binary that no longer understands them.
const STATE_SCHEMA_VERSION: &str = "v2";

/// Releases a lock only when the caller still owns it. Checking and deleting in
/// one script keeps the pair atomic — between a `GET` and a `DEL` the lock could
/// expire and be re-acquired by another task, which we would then delete.
const RELEASE_LOCK_SCRIPT: &str = r#"
if redis.call("get", KEYS[1]) == ARGV[1] then
    return redis.call("del", KEYS[1])
else
    return 0
end
"#;

#[derive(Clone)]
pub struct RedisSessionRepository {
    conn: ConnectionManager,
}

impl RedisSessionRepository {
    pub async fn new(redis_url: &str) -> AppResult<Self> {
        let client = redis::Client::open(redis_url)?;
        let conn = ConnectionManager::new(client).await?;
        Ok(Self { conn })
    }

    fn state_key(user_id: &str) -> String {
        format!("eng_os:chat_state:{STATE_SCHEMA_VERSION}:{user_id}")
    }

    fn lock_key(user_id: &str) -> String {
        format!("eng_os:lock:{user_id}")
    }

    fn event_key(event_id: &str) -> String {
        format!("eng_os:event:{event_id}")
    }

    /// `SET key value NX EX ttl`, reporting whether the key was created.
    async fn set_if_absent(&self, key: &str, value: &str, ttl_seconds: u64) -> AppResult<bool> {
        let mut conn = self.conn.clone();
        let options = SetOptions::default()
            .conditional_set(ExistenceCheck::NX)
            .with_expiration(SetExpiry::EX(ttl_seconds));

        // NX returns nil when the key already exists, so decode as Option.
        let outcome: Option<String> = conn.set_options(key, value, options).await?;
        Ok(outcome.is_some())
    }
}

impl ChatStateRepository for RedisSessionRepository {
    async fn get_state(&self, user_id: &str) -> AppResult<ChatState> {
        let mut conn = self.conn.clone();
        let key = Self::state_key(user_id);

        let payload: Option<String> = conn.get(&key).await?;

        let Some(data) = payload else {
            return Ok(ChatState::Idle);
        };

        match serde_json::from_str(&data) {
            Ok(state) => Ok(state),
            Err(error) => {
                // A payload we cannot read must not strand the learner in a
                // state they can never leave. Drop it and start clean.
                tracing::warn!(
                    %user_id,
                    %error,
                    "discarding unreadable chat state"
                );
                let _: () = conn.del(&key).await?;
                Ok(ChatState::Idle)
            }
        }
    }

    async fn set_state(&self, user_id: &str, state: &ChatState, ttl_seconds: u64) -> AppResult<()> {
        let mut conn = self.conn.clone();
        let payload = serde_json::to_string(state)?;
        let _: () = conn
            .set_ex(Self::state_key(user_id), payload, ttl_seconds)
            .await?;
        Ok(())
    }

    async fn clear_state(&self, user_id: &str) -> AppResult<()> {
        let mut conn = self.conn.clone();
        let _: () = conn.del(Self::state_key(user_id)).await?;
        Ok(())
    }

    async fn ping(&self) -> AppResult<()> {
        let mut conn = self.conn.clone();
        let pong: String = redis::cmd("PING").query_async(&mut conn).await?;
        if pong.eq_ignore_ascii_case("pong") {
            Ok(())
        } else {
            Err(AppError::Cache(redis::RedisError::from((
                redis::ErrorKind::UnexpectedReturnType,
                "unexpected PING reply",
            ))))
        }
    }
}

impl SessionLockRepository for RedisSessionRepository {
    async fn try_acquire_lock(
        &self,
        user_id: &str,
        ttl_seconds: u64,
    ) -> AppResult<Option<LockToken>> {
        let token = Uuid::new_v4().to_string();
        let acquired = self
            .set_if_absent(&Self::lock_key(user_id), &token, ttl_seconds)
            .await?;

        Ok(acquired.then(|| LockToken {
            user_id: user_id.to_string(),
            token,
        }))
    }

    async fn release_lock(&self, token: &LockToken) -> AppResult<()> {
        let mut conn = self.conn.clone();
        let _: i64 = redis::Script::new(RELEASE_LOCK_SCRIPT)
            .key(Self::lock_key(&token.user_id))
            .arg(&token.token)
            .invoke_async(&mut conn)
            .await?;
        Ok(())
    }

    async fn try_claim_event(&self, event_id: &str, ttl_seconds: u64) -> AppResult<bool> {
        self.set_if_absent(&Self::event_key(event_id), "1", ttl_seconds)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_keys_are_namespaced_and_versioned() {
        let key = RedisSessionRepository::state_key("U123");
        assert!(key.starts_with("eng_os:chat_state:"));
        assert!(
            key.contains(STATE_SCHEMA_VERSION),
            "state key must carry the schema version so old payloads are not reused"
        );
        assert!(key.ends_with("U123"));
    }

    #[test]
    fn lock_and_event_keys_do_not_collide_with_state_keys() {
        let user = "U123";
        assert_ne!(
            RedisSessionRepository::state_key(user),
            RedisSessionRepository::lock_key(user)
        );
        assert_ne!(
            RedisSessionRepository::lock_key(user),
            RedisSessionRepository::event_key(user)
        );
    }
}
