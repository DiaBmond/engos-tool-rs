use sqlx::PgPool;
use crate::application::sentence::ports::SentenceRepository;
use crate::domain::sentence::Sentence;

pub struct PostgresSentenceRepository {
    pool: PgPool,
}

impl PostgresSentenceRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl SentenceRepository for PostgresSentenceRepository {

    async fn save_sentence(&self, sentence: &Sentence) -> Result<(), String> {
        sqlx::query!(
            r#"
            INSERT INTO sentences (sentence_id, user_id, original_text, total_fix, final_feedback, is_passed)
            VALUES ($1, $2, $3, $4, $5, $6)
            ON CONFLICT (sentence_id)
            DO UPDATE SET
                total_fix = EXCLUDED.total_fix,
                final_feedback = EXCLUDED.final_feedback,
                is_passed = EXCLUDED.is_passed
            "#,
            sentence.sentence_id,
            sentence.user_id,
            sentence.original_text,
            sentence.total_fix as i16,
            sentence.final_feedback,
            sentence.is_passed
        )
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Database error while saving sentence: {}", e))?;

        Ok(())
    }
}