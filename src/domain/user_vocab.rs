use chrono::{DateTime, Utc};

/// A learner's relationship with one vocabulary word, and the basis of the
/// spaced-repetition ordering.
///
/// The previous single `guess_count` field conflated two very different things:
/// how often a word had been *shown* and how well it was *known*. They are now
/// separate, which is what makes review ordering meaningful.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserVocab {
    pub user_id: String,
    pub vocab_id: String,
    /// Times this word has been served to the learner.
    pub seen_count: u32,
    /// Times the learner recalled it correctly.
    pub correct_count: u32,
    /// Last time the learner was quizzed on it, right or wrong.
    pub last_reviewed_at: Option<DateTime<Utc>>,
}

impl UserVocab {
    /// A word being introduced for the first time.
    pub fn new(user_id: String, vocab_id: String) -> Self {
        Self {
            user_id,
            vocab_id,
            seen_count: 1,
            correct_count: 0,
            last_reviewed_at: None,
        }
    }

    pub fn from_storage(
        user_id: String,
        vocab_id: String,
        seen_count: u32,
        correct_count: u32,
        last_reviewed_at: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            user_id,
            vocab_id,
            seen_count,
            correct_count,
            last_reviewed_at,
        }
    }

    /// Considered learned once recalled correctly enough times.
    pub fn is_mastered(&self) -> bool {
        self.correct_count >= 3
    }

    /// Ratio of correct recalls to exposures, for progress display.
    pub fn accuracy(&self) -> f32 {
        if self.seen_count == 0 {
            return 0.0;
        }
        self.correct_count as f32 / self.seen_count as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_word_is_seen_once_and_never_correct() {
        let uv = UserVocab::new("U".into(), "V".into());
        assert_eq!(uv.seen_count, 1);
        assert_eq!(uv.correct_count, 0);
        assert!(uv.last_reviewed_at.is_none());
        assert!(!uv.is_mastered());
    }

    #[test]
    fn mastery_requires_three_correct_recalls() {
        let mut uv = UserVocab::new("U".into(), "V".into());
        uv.correct_count = 2;
        assert!(!uv.is_mastered());
        uv.correct_count = 3;
        assert!(uv.is_mastered());
    }

    #[test]
    fn accuracy_handles_zero_exposure() {
        let mut uv = UserVocab::new("U".into(), "V".into());
        uv.seen_count = 0;
        assert_eq!(uv.accuracy(), 0.0, "must not divide by zero");
    }
}
