use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VocabCategory {
    Daily,
    Native,
    Tech,
}

impl VocabCategory {
    /// Canonical value stored in the `vocabs.category` column. Must match the
    /// `CHECK (category IN (...))` constraint in the schema.
    pub fn as_db_str(&self) -> &'static str {
        match self {
            Self::Daily => "Daily",
            Self::Native => "Native",
            Self::Tech => "Tech",
        }
    }

    /// Thai label for chat output. Previously the handlers formatted this enum
    /// with `{:?}`, so users saw the raw Rust identifier.
    pub fn label_th(&self) -> &'static str {
        match self {
            Self::Daily => "ใช้ในชีวิตประจำวัน",
            Self::Native => "สำนวนเจ้าของภาษา",
            Self::Tech => "ศัพท์สายเทค",
        }
    }

    /// Lenient parse for values coming from the AI or from legacy rows, falling
    /// back to `Daily` rather than failing the whole round.
    pub fn from_str_lossy(s: &str) -> Self {
        s.parse().unwrap_or(Self::Daily)
    }
}

impl fmt::Display for VocabCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label_th())
    }
}

impl FromStr for VocabCategory {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "daily" => Ok(Self::Daily),
            "native" => Ok(Self::Native),
            "tech" => Ok(Self::Tech),
            _ => Err(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vocab {
    pub vocab_id: String,
    pub word: String,
    pub definition: String,
    pub category: VocabCategory,
}

impl Vocab {
    pub fn new(
        vocab_id: String,
        word: String,
        definition: String,
        category: VocabCategory,
    ) -> Self {
        Self {
            vocab_id,
            word,
            definition,
            category,
        }
    }

    /// Case- and whitespace-insensitive comparison against a learner's guess.
    /// Used as a cheap exact-match shortcut before spending an AI call.
    pub fn matches_exactly(&self, guess: &str) -> bool {
        self.word.trim().eq_ignore_ascii_case(guess.trim())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn db_str_roundtrips_through_parse() {
        for c in [
            VocabCategory::Daily,
            VocabCategory::Native,
            VocabCategory::Tech,
        ] {
            assert_eq!(VocabCategory::from_str_lossy(c.as_db_str()), c);
        }
    }

    #[test]
    fn unknown_category_falls_back_to_daily() {
        assert_eq!(
            VocabCategory::from_str_lossy("Nonsense"),
            VocabCategory::Daily
        );
    }

    #[test]
    fn parse_is_case_insensitive() {
        assert_eq!(VocabCategory::from_str_lossy("tECh"), VocabCategory::Tech);
    }

    #[test]
    fn display_uses_thai_label_not_debug_name() {
        assert_eq!(format!("{}", VocabCategory::Tech), "ศัพท์สายเทค");
    }

    #[test]
    fn exact_match_ignores_case_and_padding() {
        let v = Vocab::new(
            "1".into(),
            "Deploy".into(),
            "นำขึ้นระบบ".into(),
            VocabCategory::Tech,
        );
        assert!(v.matches_exactly("  deploy "));
        assert!(!v.matches_exactly("deployment"));
    }
}
