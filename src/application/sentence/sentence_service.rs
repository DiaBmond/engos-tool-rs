use super::ports::{DraftOutcome, SentenceAiPort, SentenceRepository, SentenceUseCase};
use crate::domain::error::AppResult;
use crate::domain::sentence::Sentence;

pub struct SentenceService<R: SentenceRepository, A: SentenceAiPort> {
    repo: R,
    ai: A,
}

impl<R: SentenceRepository, A: SentenceAiPort> SentenceService<R, A> {
    pub fn new(repo: R, ai: A) -> Self {
        Self { repo, ai }
    }
}

impl<R: SentenceRepository, A: SentenceAiPort> SentenceUseCase for SentenceService<R, A> {
    /// The row is written on **every** attempt. The original version rebuilt a
    /// fresh `Sentence` per message (resetting `total_fix` to 0) and saved only
    /// on success, so the persisted `total_fix` was always 0 and the stored
    /// `original_text` was actually the sentence that finally passed.
    async fn submit_draft(
        &self,
        sentence_id: &str,
        user_id: &str,
        draft_text: &str,
        original_text: Option<&str>,
        fix_count: u8,
    ) -> AppResult<DraftOutcome> {
        let first_draft = original_text.unwrap_or(draft_text).to_string();

        let mut sentence = Sentence::revision(
            sentence_id.to_string(),
            user_id.to_string(),
            first_draft.clone(),
            draft_text.to_string(),
            fix_count,
        );

        let analysis = self.ai.analyze_sentence(draft_text).await?;

        if analysis.is_passed {
            sentence.mark_as_passed(analysis.feedback.clone());
        } else {
            sentence.mark_as_needs_work(analysis.feedback.clone());
        }

        self.repo.save_sentence(&sentence).await?;

        Ok(DraftOutcome {
            analysis,
            total_fix: sentence.total_fix,
            original_text: first_draft,
        })
    }
}
