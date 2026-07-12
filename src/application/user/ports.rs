use std::future::Future;
use crate::domain::user::User;

pub trait UserRepository: Send + Sync {
    fn find_by_id(&self, user_id: &str) -> impl Future<Output = Result<Option<User>, String>> + Send;
    fn save(&self, user: &User) -> impl Future<Output = Result<(), String>> + Send;
}