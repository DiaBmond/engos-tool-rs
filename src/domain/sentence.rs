/// One sentence-practice attempt chain.
///
/// A draft survives across several messages: the learner writes a sentence, the
/// coach rejects it, they revise, and so on until it passes. `original_text`
/// therefore holds the *first* draft and `final_text` the most recent one — the
/// old model kept only one field and rebuilt the struct from scratch on every
/// turn, which reset `total_fix` to zero and persisted the passing sentence
/// under the name `original_text`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sentence {
    pub sentence_id: String,
    pub user_id: String,
    pub original_text: String,
    pub final_text: String,
    pub total_fix: u8,
    pub final_feedback: String,
    pub is_passed: bool,
}

impl Sentence {
    /// Starts a fresh draft chain from the learner's first attempt.
    pub fn new(sentence_id: String, user_id: String, original_text: String) -> Self {
        Self {
            sentence_id,
            user_id,
            final_text: original_text.clone(),
            original_text,
            total_fix: 0,
            final_feedback: String::new(),
            is_passed: false,
        }
    }

    /// Continues an existing chain with a revised attempt, carrying the
    /// accumulated revision count forward.
    pub fn revision(
        sentence_id: String,
        user_id: String,
        original_text: String,
        latest_text: String,
        total_fix: u8,
    ) -> Self {
        Self {
            sentence_id,
            user_id,
            original_text,
            final_text: latest_text,
            total_fix,
            final_feedback: String::new(),
            is_passed: false,
        }
    }

    /// Saturates so a determined learner cannot overflow the counter — at 255
    /// revisions this used to panic in debug builds and wrap to 0 in release.
    pub fn add_fix_count(&mut self) {
        self.total_fix = self.total_fix.saturating_add(1);
    }

    pub fn mark_as_passed(&mut self, feedback: String) {
        self.is_passed = true;
        self.final_feedback = feedback;
    }

    pub fn mark_as_needs_work(&mut self, feedback: String) {
        self.is_passed = false;
        self.final_feedback = feedback;
        self.add_fix_count();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> Sentence {
        Sentence::new("s1".into(), "u1".into(), "I has a pen".into())
    }

    #[test]
    fn new_draft_mirrors_original_into_final_text() {
        let s = draft();
        assert_eq!(s.original_text, s.final_text);
        assert_eq!(s.total_fix, 0);
        assert!(!s.is_passed);
    }

    #[test]
    fn revision_preserves_original_and_carries_fix_count() {
        let s = Sentence::revision(
            "s1".into(),
            "u1".into(),
            "I has a pen".into(),
            "I have a pen".into(),
            2,
        );
        assert_eq!(s.original_text, "I has a pen", "first draft must be kept");
        assert_eq!(s.final_text, "I have a pen");
        assert_eq!(s.total_fix, 2, "revision count must carry over");
    }

    #[test]
    fn needs_work_increments_the_persisted_counter() {
        let mut s = draft();
        s.mark_as_needs_work("ลองดูเรื่อง tense นะครับ".into());
        assert_eq!(s.total_fix, 1);
        assert!(!s.is_passed);
        assert!(!s.final_feedback.is_empty());
    }

    #[test]
    fn fix_count_saturates_instead_of_overflowing() {
        let mut s = draft();
        s.total_fix = u8::MAX;
        s.add_fix_count();
        assert_eq!(s.total_fix, u8::MAX, "must saturate, not wrap to 0");
    }

    #[test]
    fn passing_records_feedback_without_bumping_fix_count() {
        let mut s = draft();
        s.total_fix = 3;
        s.mark_as_passed("เยี่ยมครับ".into());
        assert!(s.is_passed);
        assert_eq!(s.total_fix, 3);
    }
}
