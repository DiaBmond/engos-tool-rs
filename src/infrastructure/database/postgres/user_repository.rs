use sqlx::PgPool;

use crate::application::user::ports::UserRepository;
use crate::domain::error::AppResult;
use crate::domain::user::User;

pub struct PostgresUserRepository {
    pool: PgPool,
}

impl PostgresUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserRepository for PostgresUserRepository {
    async fn find_by_id(&self, user_id: &str) -> AppResult<Option<User>> {
        let row = sqlx::query!(
            r#"
            SELECT user_id, progress_stack, current_level, created_at
            FROM users
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            // Clamp rather than cast: a stray value outside the domain range
            // would otherwise wrap silently (e.g. 70000 becoming 4464) instead
            // of saturating at the top of the range.
            User::from_storage(
                r.user_id,
                r.current_level.clamp(0, u8::MAX as i16) as u8,
                r.progress_stack.clamp(0, u16::MAX as i32) as u16,
                r.created_at,
            )
        }))
    }

    async fn save(&self, user: &User) -> AppResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO users (user_id, progress_stack, current_level, created_at)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id)
            DO UPDATE SET
                progress_stack = EXCLUDED.progress_stack,
                current_level = EXCLUDED.current_level
            "#,
            user.user_id,
            i32::from(user.progress_stack),
            i16::from(user.current_level),
            user.created_at
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn ping(&self) -> AppResult<()> {
        sqlx::query!("SELECT 1 AS ok").fetch_one(&self.pool).await?;
        Ok(())
    }
}
