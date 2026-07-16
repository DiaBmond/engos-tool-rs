use std::future::Future;
use crate::domain::vocab::Vocab;
use crate::domain::user_vocab::UserVocab;
use crate::application::vocab::dto::VocabEvaluation;

pub trait VocabRepository: Send + Sync {
    fn save_vocab(&self, vocab: &Vocab) -> impl Future<Output = Result<Vocab, String>> + Send;

    fn find_vocab_by_id(&self, vocab_id: &str) -> impl Future<Output = Result<Option<Vocab>, String>> + Send;

    fn upsert_user_vocab(&self, user_vocab: &UserVocab) -> impl Future<Output = Result<(), String>> + Send;

    fn get_review_vocabs(&self, user_id: &str, limit: usize) -> impl Future<Output = Result<Vec<(Vocab, UserVocab)>, String>> + Send;
}

pub trait VocabAiPort: Send + Sync {
    fn generate_three_vocabs(&self) -> impl Future<Output = Result<Vec<Vocab>, String>> + Send;
    
    fn evaluate_vocab_guess(&self, vocab: &Vocab, user_guess: &str) -> impl Future<Output = Result<VocabEvaluation, String>> + Send;
}