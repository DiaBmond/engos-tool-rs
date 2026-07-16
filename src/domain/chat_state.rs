use serde::{Deserialize, Serialize};
use crate::application::roleplay::dto::RoleplayScenario;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
        fix_count: u8,
    },
    
    Roleplay {
        level: u8,
        turn_count: u8,
        scenario: RoleplayScenario,
        history: Vec<(String, String)>,
    },
}

impl ChatState {
    pub fn default_state() -> Self {
        Self::Idle
    }

    pub fn is_idle(&self) -> bool {
        matches!(self, Self::Idle)
    }

    pub fn is_reviewing(&self) -> bool {
        matches!(self, Self::VocabReviewing { .. })
    }

    pub fn is_roleplay(&self) -> bool {
        matches!(self, Self::Roleplay { .. })
    }
}