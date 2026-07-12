use sqlx::PgPool;
use crate::application::vocab::ports::VocabRepository;
use crate::domain::vocab::{Vocab, VocabCategory};
use crate::domain::user_vocab::UserVocab;

pub struct PostgresVocabRepository {
    pool: PgPool,
}

impl PostgresVocabRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

impl VocabRepository for PostgresVocabRepository {
    
    async fn save_vocab(&self, vocab: &Vocab) -> Result<(), String> {
        let category_str = match vocab.category {
            VocabCategory::Daily => "Daily",
            VocabCategory::Native => "Native",
            VocabCategory::Tech => "Tech",
        };

        sqlx::query!(
            r#"
            INSERT INTO vocabs (vocab_id, word, definition, category)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (word, category) DO NOTHING
            "#,
            vocab.vocab_id,
            vocab.word,
            vocab.definition,
            category_str
        )
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Database error while saving vocab: {}", e))?;

        Ok(())
    }

    async fn upsert_user_vocab(&self, user_vocab: &UserVocab) -> Result<(), String> {
        sqlx::query!(
            r#"
            INSERT INTO user_vocabs (user_id, vocab_id, guess_count)
            VALUES ($1, $2, $3)
            ON CONFLICT (user_id, vocab_id) 
            DO UPDATE SET guess_count = user_vocabs.guess_count + 1
            "#,
            user_vocab.user_id,
            user_vocab.vocab_id,
            user_vocab.guess_count as i32
        )
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Database error while upserting user vocab: {}", e))?;

        Ok(())
    }

    async fn get_review_vocabs(&self, user_id: &str, limit: usize) -> Result<Vec<(Vocab, UserVocab)>, String> {
        let rows = sqlx::query!(
            r#"
            SELECT v.vocab_id, v.word, v.definition, v.category, uv.guess_count
            FROM user_vocabs uv
            JOIN vocabs v ON uv.vocab_id = v.vocab_id
            WHERE uv.user_id = $1
            ORDER BY uv.guess_count ASC, RANDOM()
            LIMIT $2
            "#,
            user_id,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Database error while fetching review vocabs: {}", e))?;

        let mut result = Vec::new();
        for row in rows {
            let category = match row.category.as_str() {
                "Daily" => VocabCategory::Daily,
                "Native" => VocabCategory::Native,
                "Tech" => VocabCategory::Tech,
                _ => VocabCategory::Daily,
            };

            let vocab = Vocab::new(row.vocab_id, row.word, row.definition, category);
            
            let user_vocab = UserVocab {
                user_id: user_id.to_string(),
                vocab_id: vocab.vocab_id.clone(),
                guess_count: row.guess_count as u32,
            };

            result.push((vocab, user_vocab));
        }

        Ok(result)
    }
}