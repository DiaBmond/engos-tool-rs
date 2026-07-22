use std::time::Duration;

use tokio::sync::mpsc::UnboundedReceiver;

use crate::application::usage::ports::UsageRepository;
use crate::domain::usage::UsageEvent;

/// Rows written per statement.
const BATCH_SIZE: usize = 64;

/// Longest a recorded call waits before being flushed.
const FLUSH_INTERVAL: Duration = Duration::from_secs(5);

/// Drains recorded AI calls into storage.
///
/// Runs detached from the request path so token accounting can never slow down
/// or fail a learner's turn: the producer side is an unbounded channel send that
/// cannot block, and a failed write is logged and dropped rather than retried
/// forever.
pub async fn run(mut rx: UnboundedReceiver<UsageEvent>, repo: impl UsageRepository) {
    let mut buffer: Vec<UsageEvent> = Vec::with_capacity(BATCH_SIZE);
    let mut ticker = tokio::time::interval(FLUSH_INTERVAL);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            received = rx.recv() => {
                match received {
                    Some(event) => {
                        buffer.push(event);
                        if buffer.len() >= BATCH_SIZE {
                            flush(&repo, &mut buffer).await;
                        }
                    }
                    // Channel closed: the app is shutting down. Persist what is
                    // left rather than losing it.
                    None => {
                        flush(&repo, &mut buffer).await;
                        tracing::debug!("usage writer stopped");
                        return;
                    }
                }
            }
            _ = ticker.tick() => flush(&repo, &mut buffer).await,
        }
    }
}

async fn flush(repo: &impl UsageRepository, buffer: &mut Vec<UsageEvent>) {
    if buffer.is_empty() {
        return;
    }

    match repo.record_batch(buffer).await {
        Ok(()) => tracing::debug!(count = buffer.len(), "recorded ai usage"),
        Err(error) => tracing::warn!(%error, count = buffer.len(), "failed to record ai usage"),
    }

    // Cleared either way: accounting is best-effort and must not grow unbounded
    // while the database is unavailable.
    buffer.clear();
}
