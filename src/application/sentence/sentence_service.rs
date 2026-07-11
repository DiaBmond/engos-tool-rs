use super::ports::{SentenceAiPort, SentenceRepository};
use crate::domain::sentence::Sentence;
use super::dto::SentenceAnalysisResult;

pub struct SentenceService<R: SentenceRepository, A: SentenceAiPort> {
    repo: R,
    ai: A,
}

impl<R: SentenceRepository, A: SentenceAiPort> SentenceService<R, A> {
    pub fn new(repo: R, ai: A) -> Self {
        Self { repo, ai }
    }

    pub fn start_new_draft(&self, sentence_id: &str, user_id: &str, original_text: &str) -> Sentence {
        Sentence::new(
            sentence_id.to_string(),
            user_id.to_string(),
            original_text.to_string(),
        )
    }

    pub async fn evaluate_sentence(
        &self,
        sentence: &mut Sentence,
        draft_text: &str,
    ) -> Result<SentenceAnalysisResult, String> {
        let analysis = self.ai.analyze_sentence(draft_text).await?;

        if analysis.is_passed {
            sentence.mark_as_passed(analysis.feedback.clone());

            self.repo.save_sentence(sentence).await?;
        } else {

            sentence.add_fix_count();
            
        }

        Ok(analysis)
    }
}