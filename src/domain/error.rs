use std::fmt;

/// The single error type used across every layer of the application.
///
/// Variants carry enough structure for the caller to decide *how* to react
/// (retry, give up, tell the user something specific) instead of the previous
/// stringly-typed `Result<_, String>` where every failure looked the same.
///
/// The `Display` output is meant for logs only. Anything shown to an end user
/// must go through [`AppError::user_message`], which never leaks internals.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("cache error: {0}")]
    Cache(#[from] redis::RedisError),

    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    /// The AI provider was reachable but refused or failed the request.
    #[error("ai upstream error: {0}")]
    AiUpstream(String),

    /// The AI answered, but the payload did not match the expected schema.
    #[error("ai returned malformed output: {0}")]
    AiParse(String),

    /// Failure talking to the LINE Messaging API.
    #[error("messaging error: {0}")]
    Messaging(String),

    #[error("not found: {0}")]
    NotFound(String),

    /// The persisted conversation state is inconsistent with what the handler
    /// expected (e.g. an index pointing past the end of a round).
    #[error("invalid conversation state: {0}")]
    InvalidState(String),

    #[error("configuration error: {0}")]
    Config(String),

    /// A turn exceeded its deadline. Bounded so that processing can never
    /// outlive the per-user lock that protects it.
    #[error("turn exceeded its {0}s deadline")]
    Timeout(u64),

    /// A handler panicked. Surfaced as an error so the turn still reports back
    /// to the learner instead of failing silently.
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    /// A safe, user-facing Thai message.
    ///
    /// Never interpolates the underlying error: upstream errors routinely embed
    /// request URLs, raw provider payloads and connection strings.
    pub fn user_message(&self) -> &'static str {
        match self {
            Self::AiUpstream(_) | Self::AiParse(_) => {
                "ตอนนี้ AI กำลังคิดไม่ทันครับ 🤖 ลองพิมพ์ใหม่อีกครั้งนะครับ"
            }
            Self::Database(_) | Self::Cache(_) | Self::Serialization(_) => {
                "ระบบขัดข้องชั่วคราวครับ 🛠️ กรุณาลองใหม่อีกครั้งในอีกสักครู่"
            }
            Self::InvalidState(_) => {
                "เซสชันการฝึกหมดอายุหรือขัดข้องครับ พิมพ์ \"ยกเลิก\" เพื่อกลับสู่เมนูหลักได้เลยครับ"
            }
            Self::NotFound(_) => "ไม่พบข้อมูลที่ต้องการครับ พิมพ์ \"ยกเลิก\" เพื่อเริ่มใหม่ได้เลยครับ",
            Self::Timeout(_) => "ระบบใช้เวลานานเกินไปครับ ⏱️ กรุณาส่งข้อความเดิมอีกครั้งนะครับ",
            Self::Messaging(_) | Self::Config(_) | Self::Internal(_) => {
                "เกิดข้อผิดพลาดในระบบครับ ทีมงานกำลังตรวจสอบอยู่ 🛠️"
            }
        }
    }

    /// Short, low-cardinality label for log fields and metrics.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Database(_) => "database",
            Self::Cache(_) => "cache",
            Self::Serialization(_) => "serialization",
            Self::AiUpstream(_) => "ai_upstream",
            Self::AiParse(_) => "ai_parse",
            Self::Messaging(_) => "messaging",
            Self::NotFound(_) => "not_found",
            Self::InvalidState(_) => "invalid_state",
            Self::Config(_) => "config",
            Self::Timeout(_) => "timeout",
            Self::Internal(_) => "internal",
        }
    }

    /// Whether retrying the same operation could plausibly succeed. Used to
    /// decide if a failed turn is worth surfacing as "try again".
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Database(_)
                | Self::Cache(_)
                | Self::AiUpstream(_)
                | Self::Messaging(_)
                | Self::Timeout(_)
        )
    }
}

/// Convenience alias so signatures stay readable.
pub type AppResult<T> = Result<T, AppError>;

/// Redacts anything that looks like a credential before an upstream error text
/// reaches the logs.
///
/// `reqwest::Error` embeds the full request URL in its `Display`, so a plain
/// `format!("{e}")` on a Gemini call used to print `?key=<API_KEY>` verbatim.
pub fn redact_secrets(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut rest = input;

    // Strip `key=...` / `api_key=...` / `access_token=...` query parameters.
    const SENSITIVE_PARAMS: [&str; 4] = ["key=", "api_key=", "apikey=", "access_token="];

    'outer: while !rest.is_empty() {
        for param in SENSITIVE_PARAMS {
            if let Some(pos) = rest.to_ascii_lowercase().find(param) {
                out.push_str(&rest[..pos]);
                out.push_str(param);
                out.push_str("[REDACTED]");
                let after = &rest[pos + param.len()..];
                // The secret runs until the next delimiter.
                let end = after
                    .find(['&', ' ', '"', '\'', ',', ')', '}'])
                    .unwrap_or(after.len());
                rest = &after[end..];
                continue 'outer;
            }
        }
        out.push_str(rest);
        break;
    }

    out
}

/// Wrapper that guarantees a value is never printed, even by accident.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: String) -> Self {
        Self(value)
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Secret([REDACTED])")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("[REDACTED]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_api_key_from_url() {
        let raw = "HTTP error for url (https://generativelanguage.googleapis.com/v1beta/models/gemini-2.5-flash:generateContent?key=AQ.SUPERSECRET)";
        let cleaned = redact_secrets(raw);
        assert!(
            !cleaned.contains("AQ.SUPERSECRET"),
            "secret leaked: {cleaned}"
        );
        assert!(cleaned.contains("key=[REDACTED]"));
    }

    #[test]
    fn redacts_multiple_params() {
        let cleaned = redact_secrets("a?key=one&b&access_token=two end");
        assert!(!cleaned.contains("one"));
        assert!(!cleaned.contains("two"));
    }

    #[test]
    fn leaves_clean_text_untouched() {
        let raw = "connection refused (os error 61)";
        assert_eq!(redact_secrets(raw), raw);
    }

    #[test]
    fn secret_never_prints_inner_value() {
        let s = Secret::new("hunter2".to_string());
        assert_eq!(format!("{s}"), "[REDACTED]");
        assert_eq!(format!("{s:?}"), "Secret([REDACTED])");
        assert_eq!(s.expose(), "hunter2");
    }
}
