use serde::{Deserialize, Serialize};

use crate::application::roleplay::dto::RoleplayScenario;

/// Vocabulary words served per guessing round.
pub const VOCAB_ROUND_SIZE: usize = 3;

/// Wrong guesses allowed on one word before the answer is revealed and the
/// round moves on. Without a ceiling a learner could guess forever, and every
/// wrong guess costs one AI call.
pub const MAX_VOCAB_ATTEMPTS: u8 = 3;

/// Conversational turns in a roleplay session.
///
/// The learner sends exactly this many messages and receives a reply to each;
/// the final reply carries the evaluation. The previous arrangement announced
/// five turns but produced only four replies, because the last message went
/// straight to grading.
pub const ROLEPLAY_TOTAL_TURNS: u8 = 10;

/// How long an idle conversation keeps its state, in seconds.
pub const STATE_TTL_SECONDS: u64 = 3600;

/// One exchange in a roleplay session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleplayTurn {
    pub user_message: String,
    pub ai_message: String,
}

/// Where a learner currently is in the conversation.
///
/// Persisted to Redis as JSON. The shape is part of the storage contract, so
/// [`crate::infrastructure::database::redis_repo`] namespaces its keys with a
/// schema version and treats an unparseable payload as `Idle` rather than
/// failing the turn.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChatState {
    Idle,

    VocabGuessing {
        vocab_ids: Vec<String>,
        current_index: usize,
        attempt: u8,
    },

    VocabReviewing {
        review_list: Vec<String>,
        current_index: usize,
    },

    SentenceDraft {
        sentence_id: Option<String>,
        /// First draft, kept so the persisted row records what the learner
        /// originally wrote rather than the sentence that finally passed.
        original_text: Option<String>,
        fix_count: u8,
    },

    Roleplay {
        turn_count: u8,
        scenario: RoleplayScenario,
        history: Vec<RoleplayTurn>,
    },

    /// Waiting for the learner to confirm erasure of their account.
    /// Destructive and irreversible, so it is never a single keystroke.
    ConfirmDeletion,
}

impl ChatState {
    /// Low-cardinality name for structured log fields.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::VocabGuessing { .. } => "vocab_guessing",
            Self::VocabReviewing { .. } => "vocab_reviewing",
            Self::SentenceDraft { .. } => "sentence_draft",
            Self::Roleplay { .. } => "roleplay",
            Self::ConfirmDeletion => "confirm_deletion",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scenario() -> RoleplayScenario {
        RoleplayScenario {
            role_name: "John".into(),
            setting: "ร้านกาแฟ".into(),
            opening_line: "Hi there!".into(),
        }
    }

    /// The state shape is a storage contract with Redis — a silent change to it
    /// would strand every in-flight session.
    #[test]
    fn every_variant_survives_a_json_roundtrip() {
        let states = vec![
            ChatState::Idle,
            ChatState::VocabGuessing {
                vocab_ids: vec!["a".into(), "b".into(), "c".into()],
                current_index: 1,
                attempt: 2,
            },
            ChatState::VocabReviewing {
                review_list: vec!["a".into()],
                current_index: 0,
            },
            ChatState::SentenceDraft {
                sentence_id: Some("s1".into()),
                original_text: Some("I has a pen".into()),
                fix_count: 4,
            },
            ChatState::Roleplay {
                turn_count: 3,
                scenario: scenario(),
                history: vec![RoleplayTurn {
                    user_message: "Hello".into(),
                    ai_message: "Hi!".into(),
                }],
            },
            ChatState::ConfirmDeletion,
        ];

        for state in states {
            let json = serde_json::to_string(&state).expect("serialize");
            let back: ChatState = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(state, back, "roundtrip changed the state");
        }
    }

    #[test]
    fn names_are_stable_for_logging() {
        assert_eq!(ChatState::Idle.name(), "idle");
        assert_eq!(
            ChatState::Roleplay {
                turn_count: 1,
                scenario: scenario(),
                history: vec![],
            }
            .name(),
            "roleplay"
        );
    }
}
