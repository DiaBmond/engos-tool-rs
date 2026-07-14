use std::future::Future;
use crate::domain::user::User;
use crate::domain::chat_state::ChatState;

pub trait UserRepository: Send + Sync {
    fn find_by_id(&self, user_id: &str) -> impl Future<Output = Result<Option<User>, String>> + Send;
    fn save(&self, user: &User) -> impl Future<Output = Result<(), String>> + Send;
}

pub trait ChatStateRepository: Send + Sync {
    fn get_state(&self, user_id: &str) -> impl Future<Output = Result<ChatState, String>> + Send;
    fn set_state(&self, user_id: &str, state: &ChatState, ttl_seconds: u64) -> impl Future<Output = Result<(), String>> + Send;
    fn clear_state(&self, user_id: &str) -> impl Future<Output = Result<(), String>> + Send;
}