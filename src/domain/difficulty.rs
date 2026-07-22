//! How a learner's level translates into difficulty for each mode.
//!
//! `current_level` used to reach exactly one place — roleplay scenario
//! generation. Vocabulary and sentence coaching ignored it entirely, so a
//! level 1 and a level 4 learner received identically hard words and were
//! marked against identical standards. Levelling up changed almost nothing,
//! which made the whole progression loop decorative.

use crate::domain::user::MAX_LEVEL;

/// Guidance injected into the vocabulary generation prompt.
pub fn vocab_guidance(level: u8) -> &'static str {
    match level.clamp(1, MAX_LEVEL) {
        1 => {
            "Beginner (CEFR A2). Choose high-frequency everyday words a learner meets in their first year of English, and the most common beginner-level programming terms (e.g. \"bug\", \"deploy\", \"merge\")."
        }
        2 => {
            "Intermediate (CEFR B1). Choose words common in workplace conversation, plus everyday software engineering vocabulary used in stand-ups and code review (e.g. \"refactor\", \"rollback\", \"trade-off\")."
        }
        3 => {
            "Advanced (CEFR B2-C1). Choose precise, less common words, natural idioms, and technical vocabulary used in design discussions and incident reviews (e.g. \"mitigate\", \"bottleneck\", \"regression\")."
        }
        _ => {
            "Near-native (CEFR C1-C2). Choose nuanced vocabulary, subtle idioms and register-sensitive expressions, plus specialised engineering terminology used in architecture and post-mortem writing (e.g. \"idempotent\", \"contention\", \"attrition\")."
        }
    }
}

/// Grading standard injected into the sentence analysis prompt.
pub fn sentence_guidance(level: u8) -> &'static str {
    match level.clamp(1, MAX_LEVEL) {
        1 => {
            "The user is a beginner. Pass the sentence if the meaning is clear, even with awkward phrasing. Only fail it for errors that genuinely obscure meaning. Be warm and encouraging."
        }
        2 => {
            "The user is intermediate. Expect correct basic tense, articles and subject-verb agreement. Fail the sentence for repeated grammar errors, but tolerate slightly unnatural phrasing."
        }
        3 => {
            "The user is advanced. Expect accurate grammar and reasonably natural phrasing. Fail the sentence if it reads as translated rather than written in English, or if word choice is noticeably off."
        }
        _ => {
            "The user is near-native. Hold them to a professional standard: natural collocation, appropriate register, and precise word choice. Fail anything a native colleague would quietly rewrite before sending."
        }
    }
}

/// Scenario brief injected into the roleplay director prompt.
///
/// Every level is given a software-engineering setting: that angle is the whole
/// point of the tool, and burying it behind higher levels meant most sessions
/// never reached it.
pub fn roleplay_guidance(level: u8) -> &'static str {
    match level.clamp(1, MAX_LEVEL) {
        1 => {
            "Level 1 (Beginner): A short, low-pressure workplace exchange with simple vocabulary and short sentences — greeting a new teammate, giving a one-line stand-up update, or asking a colleague for help finding a file."
        }
        2 => {
            "Level 2 (Intermediate): A routine engineering conversation requiring some problem-solving — walking a teammate through a bug you found, asking for clarification on a ticket, or discussing a small code review comment."
        }
        3 => {
            "Level 3 (Advanced): A high-confidence situation requiring explanation and negotiation — a technical job interview, pushing back on a deadline, defending a design decision, or explaining a trade-off to a product manager."
        }
        _ => {
            "Level 4 (Native/Master): High-pressure communication under scrutiny — explaining a production outage to executives, leading a blameless post-mortem, or negotiating scope with a difficult client."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_level_has_distinct_guidance() {
        for guidance in [
            vocab_guidance as fn(u8) -> &'static str,
            sentence_guidance,
            roleplay_guidance,
        ] {
            let texts: Vec<&str> = (1..=MAX_LEVEL).map(guidance).collect();
            for i in 0..texts.len() {
                for j in (i + 1)..texts.len() {
                    assert_ne!(
                        texts[i],
                        texts[j],
                        "levels {} and {} share guidance, so levelling up changes nothing",
                        i + 1,
                        j + 1
                    );
                }
            }
        }
    }

    #[test]
    fn out_of_range_levels_are_clamped_not_panicking() {
        assert_eq!(vocab_guidance(0), vocab_guidance(1));
        assert_eq!(vocab_guidance(99), vocab_guidance(MAX_LEVEL));
        assert_eq!(sentence_guidance(0), sentence_guidance(1));
        assert_eq!(roleplay_guidance(99), roleplay_guidance(MAX_LEVEL));
    }

    /// The developer angle is the product's only real differentiator, so it must
    /// be present from the very first level rather than unlocked later.
    #[test]
    fn engineering_context_is_present_at_every_roleplay_level() {
        for level in 1..=MAX_LEVEL {
            let text = roleplay_guidance(level).to_lowercase();
            assert!(
                [
                    "stand-up",
                    "teammate",
                    "bug",
                    "ticket",
                    "code review",
                    "technical",
                    "design",
                    "production",
                    "post-mortem",
                    "engineering"
                ]
                .iter()
                .any(|kw| text.contains(kw)),
                "level {level} has no engineering context"
            );
        }
    }
}
