use super::ports::{UserRepository, UserUseCase};
use crate::domain::error::AppResult;
use crate::domain::user::User;

/// Owns learner lifecycle and progression.
pub struct UserService<R: UserRepository> {
    repo: R,
}

impl<R: UserRepository> UserService<R> {
    pub fn new(repo: R) -> Self {
        Self { repo }
    }
}

impl<R: UserRepository> UserUseCase for UserService<R> {
    async fn get_or_create(&self, user_id: &str) -> AppResult<User> {
        if let Some(user) = self.repo.find_by_id(user_id).await? {
            return Ok(user);
        }

        let new_user = User::new(user_id.to_string());
        self.repo.save(&new_user).await?;
        Ok(new_user)
    }

    async fn award_progress(&self, user: &mut User) -> AppResult<bool> {
        let levelled_up = user.award_progress();
        self.repo.save(user).await?;
        Ok(levelled_up)
    }

    async fn penalize(&self, user: &mut User) -> AppResult<()> {
        user.penalize();
        self.repo.save(user).await
    }

    async fn health_check(&self) -> AppResult<()> {
        self.repo.ping().await
    }
}
