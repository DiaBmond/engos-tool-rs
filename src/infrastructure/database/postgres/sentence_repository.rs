use sqlx::PgPool;

use crate::application::sentence::ports::SentenceRepository;
use crate::domain::error::AppResult;
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
    async fn save_sentence(&self, sentence: &Sentence) -> AppResult<()> {
        // `original_text` is deliberately absent from the UPDATE list: it holds
        // the learner's first draft and must survive every later revision.
        sqlx::query!(
            r#"
            INSERT INTO sentences (
                sentence_id, user_id, original_text, final_text,
                total_fix, final_feedback, is_passed
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7)
            ON CONFLICT (sentence_id)
            DO UPDATE SET
                final_text = EXCLUDED.final_text,
                total_fix = EXCLUDED.total_fix,
                final_feedback = EXCLUDED.final_feedback,
                is_passed = EXCLUDED.is_passed,
                updated_at = NOW()
            "#,
            sentence.sentence_id,
            sentence.user_id,
            sentence.original_text,
            sentence.final_text,
            i16::from(sentence.total_fix),
            sentence.final_feedback,
            sentence.is_passed
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }
}
