use std::future::Future;

use crate::domain::error::AppResult;

/// Outbound chat transport.
///
/// Lives in the application layer, not next to the LINE client, so handlers
/// depend on this abstraction rather than on a concrete HTTP client. The
/// previous version declared an equivalent trait inside the infrastructure
/// adapter and then never used it — handlers called the concrete type directly,
/// which made them impossible to test without real network calls.
pub trait MessagingPort: Send + Sync {
    /// Replies using a reply token. Tokens are single-use and short-lived.
    fn reply_text(
        &self,
        reply_token: &str,
        text: &str,
    ) -> impl Future<Output = AppResult<()>> + Send;

    /// Sends an unsolicited message, used when a reply token is spent.
    fn push_text(&self, user_id: &str, text: &str) -> impl Future<Output = AppResult<()>> + Send;

    /// Replies if possible, otherwise pushes.
    ///
    /// Processing happens after the webhook has already been acknowledged, so a
    /// slow AI call can outlive the reply token. Falling back to a push keeps
    /// the learner from silently receiving nothing.
    fn respond(
        &self,
        reply_token: &str,
        user_id: &str,
        text: &str,
    ) -> impl Future<Output = AppResult<()>> + Send {
        async move {
            match self.reply_text(reply_token, text).await {
                Ok(()) => Ok(()),
                Err(error) => {
                    tracing::warn!(%error, "reply token unusable, falling back to push");
                    self.push_text(user_id, text).await
                }
            }
        }
    }
}
