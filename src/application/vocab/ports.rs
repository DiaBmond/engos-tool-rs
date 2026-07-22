use std::future::Future;

use crate::application::vocab::dto::VocabEvaluation;
use crate::domain::error::AppResult;
use crate::domain::user_vocab::UserVocab;
use crate::domain::vocab::Vocab;

/// Driven port: vocabulary persistence.
pub trait VocabRepository: Send + Sync {
    /// Inserts a word, or returns the existing row when the same
    /// `(word, category)` pair is already known.
    fn save_vocab(&self, vocab: &Vocab) -> impl Future<Output = AppResult<Vocab>> + Send;

    /// Persists a whole round and registers every word against the learner in
    /// **one transaction**.
    ///
    /// Doing this word-by-word left a partially registered round behind if any
    /// step failed, so the learner's state referenced words that were never
    /// linked to them.
    fn register_round(
        &self,
        user_id: &str,
        vocabs: &[Vocab],
    ) -> impl Future<Output = AppResult<Vec<Vocab>>> + Send;

    fn find_vocab_by_id(
        &self,
        vocab_id: &str,
    ) -> impl Future<Output = AppResult<Option<Vocab>>> + Send;

    /// Registers a word as served to a learner, incrementing `seen_count`.
    fn upsert_user_vocab(
        &self,
        user_vocab: &UserVocab,
    ) -> impl Future<Output = AppResult<()>> + Send;

    /// Records the outcome of a quiz so review ordering can adapt.
    /// Always stamps `last_reviewed_at`; only a correct recall raises
    /// `correct_count`.
    fn record_review_outcome(
        &self,
        user_id: &str,
        vocab_id: &str,
        was_correct: bool,
    ) -> impl Future<Output = AppResult<()>> + Send;

    /// Weakest-known words first. Returns an empty vector when the learner has
    /// nothing to review — that is a normal outcome, not an error.
    fn get_review_vocabs(
        &self,
        user_id: &str,
        limit: usize,
    ) -> impl Future<Output = AppResult<Vec<(Vocab, UserVocab)>>> + Send;

    /// Words the learner has recalled correctly often enough to be considered
    /// known, used to steer generation away from them.
    fn get_mastered_words(
        &self,
        user_id: &str,
        limit: usize,
    ) -> impl Future<Output = AppResult<Vec<String>>> + Send;
}

/// Driven port: vocabulary generation and grading.
pub trait VocabAiPort: Send + Sync {
    /// `avoid` lists words the learner has already mastered, so the model does
    /// not keep serving what they know.
    fn generate_three_vocabs(
        &self,
        level: u8,
        avoid: &[String],
    ) -> impl Future<Output = AppResult<Vec<Vocab>>> + Send;

    fn evaluate_vocab_guess(
        &self,
        vocab: &Vocab,
        user_guess: &str,
    ) -> impl Future<Output = AppResult<VocabEvaluation>> + Send;
}

/// Driving port: what the transport layer may do with vocabulary.
pub trait VocabUseCase: Send + Sync {
    fn start_new_round(
        &self,
        user_id: &str,
        level: u8,
    ) -> impl Future<Output = AppResult<Vec<Vocab>>> + Send;

    /// Fetches a word the conversation state refers to. A missing id means the
    /// stored state no longer matches the database.
    fn get_vocab(&self, vocab_id: &str) -> impl Future<Output = AppResult<Vocab>> + Send;

    fn check_answer(
        &self,
        target: &Vocab,
        user_answer: &str,
    ) -> impl Future<Output = AppResult<VocabEvaluation>> + Send;

    fn record_answer(
        &self,
        user_id: &str,
        vocab_id: &str,
        was_correct: bool,
    ) -> impl Future<Output = AppResult<()>> + Send;

    fn get_review_vocabs(
        &self,
        user_id: &str,
    ) -> impl Future<Output = AppResult<Vec<(Vocab, UserVocab)>>> + Send;
}
