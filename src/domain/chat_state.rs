use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ChatState {
    Idle,
    
    VocabGuessing {
        target_vocab_id: String,
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
}