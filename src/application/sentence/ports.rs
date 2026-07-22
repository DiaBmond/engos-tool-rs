use std::future::Future;

use super::dto::SentenceAnalysisResult;
use crate::domain::error::AppResult;
use crate::domain::sentence::Sentence;

/// Driven port: sentence persistence.
pub trait SentenceRepository: Send + Sync {
    /// Upserts the draft chain. Called on every attempt, not just the passing
    /// one, so `total_fix` reflects the real number of revisions.
    fn save_sentence(&self, sentence: &Sentence) -> impl Future<Output = AppResult<()>> + Send;
}

/// Driven port: sentence coaching.
pub trait SentenceAiPort: Send + Sync {
    fn analyze_sentence(
        &self,
        current_text: &str,
    ) -> impl Future<Output = AppResult<SentenceAnalysisResult>> + Send;
}

/// Outcome of one submitted draft, including the revision count the caller must
/// carry forward into the conversation state.
#[derive(Debug, Clone)]
pub struct DraftOutcome {
    pub analysis: SentenceAnalysisResult,
    /// Revisions accumulated so far, after this attempt.
    pub total_fix: u8,
    /// The learner's first draft in this chain.
    pub original_text: String,
}

/// Driving port: what the transport layer may do with sentence drafts.
pub trait SentenceUseCase: Send + Sync {
    /// Grades one submitted draft and persists the chain.
    ///
    /// `original_text` is `None` on the first message of a chain and carries the
    /// first draft on every revision; `fix_count` is the revisions made so far.
    /// Both come from the conversation state.
    fn submit_draft(
        &self,
        sentence_id: &str,
        user_id: &str,
        draft_text: &str,
        original_text: Option<&str>,
        fix_count: u8,
    ) -> impl Future<Output = AppResult<DraftOutcome>> + Send;
}
