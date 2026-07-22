use chrono::{DateTime, Utc};

/// Highest level a learner can reach.
pub const MAX_LEVEL: u8 = 4;

/// Successful sessions required to advance one level.
pub const STACK_TO_LEVEL_UP: u16 = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct User {
    pub user_id: String,
    pub current_level: u8,
    pub progress_stack: u16,
    pub created_at: DateTime<Utc>,
}

impl User {
    pub fn new(user_id: String) -> Self {
        Self {
            user_id,
            current_level: 1,
            progress_stack: 0,
            created_at: Utc::now(),
        }
    }

    /// Rebuilds a user loaded from storage, clamping persisted values into the
    /// valid domain range so a bad row can never produce an illegal state.
    pub fn from_storage(
        user_id: String,
        current_level: u8,
        progress_stack: u16,
        created_at: DateTime<Utc>,
    ) -> Self {
        Self {
            user_id,
            current_level: current_level.clamp(1, MAX_LEVEL),
            progress_stack,
            created_at,
        }
    }

    /// Records one successful practice session, whichever mode produced it.
    ///
    /// This is the single place progress is granted. Every mode must call it so
    /// the level-up rule stays consistent — previously the vocab mode did a raw
    /// `progress_stack += 1` and skipped the level check entirely, letting the
    /// stack grow unbounded and then trigger an instant level-up on the next
    /// roleplay win.
    ///
    /// Returns `true` when this award caused a level-up.
    pub fn award_progress(&mut self) -> bool {
        if self.is_max_level() {
            // Already at the ceiling: nothing to accumulate toward.
            return false;
        }

        self.progress_stack = self.progress_stack.saturating_add(1);

        if self.progress_stack >= STACK_TO_LEVEL_UP {
            self.current_level = (self.current_level + 1).min(MAX_LEVEL);
            self.progress_stack = 0;
            return true;
        }

        false
    }

    /// Records a failed session. Saturates at zero rather than wrapping.
    pub fn penalize(&mut self) {
        self.progress_stack = self.progress_stack.saturating_sub(1);
    }

    pub fn is_max_level(&self) -> bool {
        self.current_level >= MAX_LEVEL
    }

    /// Sessions still needed before the next level-up.
    pub fn progress_remaining(&self) -> u16 {
        if self.is_max_level() {
            0
        } else {
            STACK_TO_LEVEL_UP.saturating_sub(self.progress_stack)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user() -> User {
        User::new("U-test".to_string())
    }

    #[test]
    fn new_user_starts_at_level_one() {
        let u = user();
        assert_eq!(u.current_level, 1);
        assert_eq!(u.progress_stack, 0);
    }

    #[test]
    fn levels_up_exactly_at_threshold_and_resets_stack() {
        let mut u = user();
        for _ in 0..(STACK_TO_LEVEL_UP - 1) {
            assert!(!u.award_progress(), "should not level up early");
        }
        assert_eq!(u.progress_stack, STACK_TO_LEVEL_UP - 1);
        assert!(u.award_progress(), "should level up on the threshold award");
        assert_eq!(u.current_level, 2);
        assert_eq!(u.progress_stack, 0, "stack must reset after level up");
    }

    #[test]
    fn stops_at_max_level_and_does_not_accumulate() {
        let mut u = user();
        for _ in 0..100 {
            u.award_progress();
        }
        assert_eq!(u.current_level, MAX_LEVEL);
        assert_eq!(
            u.progress_stack, 0,
            "stack must not grow once the ceiling is reached"
        );
        assert!(!u.award_progress());
    }

    #[test]
    fn penalize_saturates_at_zero() {
        let mut u = user();
        u.penalize();
        u.penalize();
        assert_eq!(u.progress_stack, 0, "must not underflow");
    }

    #[test]
    fn award_does_not_overflow() {
        let mut u = user();
        u.current_level = MAX_LEVEL;
        u.progress_stack = u16::MAX;
        u.award_progress();
        assert_eq!(u.progress_stack, u16::MAX, "must saturate, not wrap");
    }

    #[test]
    fn from_storage_clamps_out_of_range_level() {
        let u = User::from_storage("U".into(), 99, 0, Utc::now());
        assert_eq!(u.current_level, MAX_LEVEL);
        let u = User::from_storage("U".into(), 0, 0, Utc::now());
        assert_eq!(u.current_level, 1);
    }

    #[test]
    fn progress_remaining_reports_gap_to_next_level() {
        let mut u = user();
        assert_eq!(u.progress_remaining(), STACK_TO_LEVEL_UP);
        u.award_progress();
        assert_eq!(u.progress_remaining(), STACK_TO_LEVEL_UP - 1);
    }
}
