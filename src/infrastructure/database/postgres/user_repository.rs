use sqlx::PgPool;
use crate::domain::user::User;
use crate::application::user::ports::UserRepository;

pub struct PostgresUserRepository {
    pool: PgPool,
}

impl PostgresUserRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl UserRepository for PostgresUserRepository {

    async fn find_by_id(&self, user_id: &str) -> Result<Option<User>, String> {
        let row = sqlx::query!(
            r#"
            SELECT user_id, progress_stack, current_level, created_at
            FROM users
            WHERE user_id = $1
            "#,
            user_id
        )
        .fetch_optional(&self.pool)
        .await
        .map_err(|e| format!("Database error while fetching user: {}", e))?;

        if let Some(r) = row {
            Ok(Some(User {
                user_id: r.user_id,
                progress_stack: r.progress_stack as u8,
                current_level: r.current_level as u8,
                created_at: r.created_at.unwrap_or_else(chrono::Utc::now), 
            }))
        } else {
            Ok(None)
        }
    }

    async fn save(&self, user: &User) -> Result<(), String> {
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
            user.progress_stack as i16,
            user.current_level as i16,
            user.created_at 
        )
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Database error while saving user: {}", e))?;

        Ok(())
    }
}