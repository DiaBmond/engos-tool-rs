use sqlx::{PgPool, Postgres, Transaction};

use crate::application::vocab::ports::VocabRepository;
use crate::domain::error::AppResult;
use crate::domain::user_vocab::UserVocab;
use crate::domain::vocab::{Vocab, VocabCategory};

pub struct PostgresVocabRepository {
    pool: PgPool,
}

impl PostgresVocabRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Inserts (or reuses) one word inside an open transaction.
    async fn save_vocab_tx(tx: &mut Transaction<'_, Postgres>, vocab: &Vocab) -> AppResult<Vocab> {
        let row = sqlx::query!(
            r#"
            INSERT INTO vocabs (vocab_id, word, definition, category)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (word, category) DO UPDATE
            SET definition = EXCLUDED.definition
            RETURNING vocab_id, word, definition, category
            "#,
            vocab.vocab_id,
            vocab.word,
            vocab.definition,
            vocab.category.as_db_str()
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(Vocab::new(
            row.vocab_id,
            row.word,
            row.definition,
            VocabCategory::from_str_lossy(&row.category),
        ))
    }
}

impl VocabRepository for PostgresVocabRepository {
    async fn save_vocab(&self, vocab: &Vocab) -> AppResult<Vocab> {
        // The `DO UPDATE` (rather than `DO NOTHING`) matters: `RETURNING` only
        // yields a row when one is actually written, so `DO NOTHING` would
        // return nothing for an already-known word and break the round.
        let row = sqlx::query!(
            r#"
            INSERT INTO vocabs (vocab_id, word, definition, category)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (word, category) DO UPDATE
            SET definition = EXCLUDED.definition
            RETURNING vocab_id, word, definition, category
            "#,
            vocab.vocab_id,
            vocab.word,
            vocab.definition,
            vocab.category.as_db_str()
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(Vocab::new(
            row.vocab_id,
            row.word,
            row.definition,
            VocabCategory::from_str_lossy(&row.category),
        ))
    }

    async fn register_round(&self, user_id: &str, vocabs: &[Vocab]) -> AppResult<Vec<Vocab>> {
        let mut tx = self.pool.begin().await?;
        let mut round = Vec::with_capacity(vocabs.len());

        for vocab in vocabs {
            // A word already in the library keeps its existing id, so always
            // carry the persisted row forward — the ids stored in the chat
            // state must resolve later.
            let saved = Self::save_vocab_tx(&mut tx, vocab).await?;

            sqlx::query!(
                r#"
                INSERT INTO user_vocabs (user_id, vocab_id, seen_count, correct_count)
                VALUES ($1, $2, 1, 0)
                ON CONFLICT (user_id, vocab_id)
                DO UPDATE SET seen_count = user_vocabs.seen_count + 1
                "#,
                user_id,
                saved.vocab_id
            )
            .execute(&mut *tx)
            .await?;

            round.push(saved);
        }

        tx.commit().await?;
        Ok(round)
    }

    async fn upsert_user_vocab(&self, user_vocab: &UserVocab) -> AppResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO user_vocabs (user_id, vocab_id, seen_count, correct_count)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (user_id, vocab_id)
            DO UPDATE SET seen_count = user_vocabs.seen_count + 1
            "#,
            user_vocab.user_id,
            user_vocab.vocab_id,
            user_vocab.seen_count as i32,
            user_vocab.correct_count as i32
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn record_review_outcome(
        &self,
        user_id: &str,
        vocab_id: &str,
        was_correct: bool,
    ) -> AppResult<()> {
        // Stamping `last_reviewed_at` on wrong answers too is what stops a word
        // the learner keeps missing from being served on every single round.
        sqlx::query!(
            r#"
            UPDATE user_vocabs
            SET correct_count = correct_count + CASE WHEN $3 THEN 1 ELSE 0 END,
                last_reviewed_at = NOW()
            WHERE user_id = $1 AND vocab_id = $2
            "#,
            user_id,
            vocab_id,
            was_correct
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    async fn get_review_vocabs(
        &self,
        user_id: &str,
        limit: usize,
    ) -> AppResult<Vec<(Vocab, UserVocab)>> {
        let rows = sqlx::query!(
            r#"
            SELECT v.vocab_id, v.word, v.definition, v.category,
                   uv.seen_count, uv.correct_count, uv.last_reviewed_at
            FROM user_vocabs uv
            JOIN vocabs v ON uv.vocab_id = v.vocab_id
            WHERE uv.user_id = $1
            ORDER BY uv.correct_count ASC,
                     uv.last_reviewed_at ASC NULLS FIRST,
                     RANDOM()
            LIMIT $2
            "#,
            user_id,
            limit as i64
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| {
                let vocab = Vocab::new(
                    row.vocab_id,
                    row.word,
                    row.definition,
                    VocabCategory::from_str_lossy(&row.category),
                );
                let user_vocab = UserVocab::from_storage(
                    user_id.to_string(),
                    vocab.vocab_id.clone(),
                    row.seen_count.max(0) as u32,
                    row.correct_count.max(0) as u32,
                    row.last_reviewed_at,
                );
                (vocab, user_vocab)
            })
            .collect())
    }

    async fn find_vocab_by_id(&self, vocab_id: &str) -> AppResult<Option<Vocab>> {
        let row = sqlx::query!(
            r#"
            SELECT vocab_id, word, definition, category
            FROM vocabs
            WHERE vocab_id = $1
            "#,
            vocab_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| {
            Vocab::new(
                r.vocab_id,
                r.word,
                r.definition,
                VocabCategory::from_str_lossy(&r.category),
            )
        }))
    }
}
