use super::dto::VocabEvaluation;
use super::ports::{VocabAiPort, VocabRepository, VocabUseCase};
use crate::domain::chat_state::VOCAB_ROUND_SIZE;
use crate::domain::error::{AppError, AppResult};
use crate::domain::user_vocab::UserVocab;
use crate::domain::vocab::Vocab;

/// Words offered in one review session.
const REVIEW_BATCH_SIZE: usize = 10;

pub struct VocabService<R: VocabRepository, A: VocabAiPort> {
    repo: R,
    ai: A,
}

impl<R: VocabRepository, A: VocabAiPort> VocabService<R, A> {
    pub fn new(repo: R, ai: A) -> Self {
        Self { repo, ai }
    }
}

impl<R: VocabRepository, A: VocabAiPort> VocabUseCase for VocabService<R, A> {
    async fn start_new_round(&self, user_id: &str) -> AppResult<Vec<Vocab>> {
        let generated = self.ai.generate_three_vocabs().await?;

        if generated.len() != VOCAB_ROUND_SIZE {
            return Err(AppError::AiParse(format!(
                "expected {VOCAB_ROUND_SIZE} vocabulary words, model returned {}",
                generated.len()
            )));
        }

        // One transaction: either the whole round is registered or none of it.
        // Note the returned ids may differ from the generated ones, because a
        // word already in the library keeps its existing row.
        self.repo.register_round(user_id, &generated).await
    }

    async fn get_vocab(&self, vocab_id: &str) -> AppResult<Vocab> {
        self.repo
            .find_vocab_by_id(vocab_id)
            .await?
            .ok_or_else(|| AppError::InvalidState(format!("vocab {vocab_id} no longer exists")))
    }

    async fn check_answer(&self, target: &Vocab, user_answer: &str) -> AppResult<VocabEvaluation> {
        // Exact matches skip the AI call entirely: it is the common case and
        // costs nothing to detect.
        if target.matches_exactly(user_answer) {
            return Ok(VocabEvaluation {
                is_correct: true,
                feedback: format!(
                    "ตอบถูกเป๊ะเลยครับ! \"{}\" แปลว่า {}",
                    target.word, target.definition
                ),
            });
        }

        self.ai.evaluate_vocab_guess(target, user_answer).await
    }

    async fn record_answer(
        &self,
        user_id: &str,
        vocab_id: &str,
        was_correct: bool,
    ) -> AppResult<()> {
        self.repo
            .record_review_outcome(user_id, vocab_id, was_correct)
            .await
    }

    async fn get_review_vocabs(&self, user_id: &str) -> AppResult<Vec<(Vocab, UserVocab)>> {
        self.repo
            .get_review_vocabs(user_id, REVIEW_BATCH_SIZE)
            .await
    }
}
