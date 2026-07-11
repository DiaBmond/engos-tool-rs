use super::ports::{VocabAiPort, VocabRepository};
use crate::domain::user_vocab::UserVocab;
use crate::domain::vocab::Vocab;

pub struct VocabService<R: VocabRepository, A: VocabAiPort> {
    repo: R,
    ai: A,
}

impl<R: VocabRepository, A: VocabAiPort> VocabService<R, A> {
    pub fn new(repo: R, ai: A) -> Self {
        Self { repo, ai }
    }

    pub async fn start_new_round(&self) -> Result<Vec<Vocab>, String> {
        let vocabs = self.ai.generate_three_vocabs().await?;
        
        if vocabs.len() != 3 {
            return Err("The AI ​​didn't generate all three words as specified.".to_string());
        }

        Ok(vocabs)
    }

    pub fn check_answer(&self, target_word: &str, user_answer: &str) -> bool {
        target_word.trim().eq_ignore_ascii_case(user_answer.trim())
    }

    pub async fn save_completed_round(
        &self,
        user_id: &str,
        completed_vocabs: Vec<Vocab>,
    ) -> Result<(), String> {
        for vocab in completed_vocabs {
            self.repo.save_vocab(&vocab).await?;

            let user_vocab = UserVocab::new(user_id.to_string(), vocab.vocab_id.clone());
            self.repo.upsert_user_vocab(&user_vocab).await?;
        }

        Ok(())
    }

    pub async fn get_review_vocabs(&self, user_id: &str) -> Result<Vec<(Vocab, UserVocab)>, String> {
        let review_list = self.repo.get_review_vocabs(user_id, 10).await?;
        
        if review_list.is_empty() {
            return Err("There are no vocabulary words in the library to review yet. Please try a regular round first!".to_string());
        }
        
        Ok(review_list)
    }

    pub async fn update_review_success(&self, mut user_vocab: UserVocab) -> Result<(), String> {
        user_vocab.add_guess_count();
        self.repo.upsert_user_vocab(&user_vocab).await
    }
}