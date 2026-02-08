//! Background worker that batches and flushes [`Saveable`] items to storage.
//!
//! This module is internal -- users interact with it indirectly through
//! [`HtmlSaverHandle`](crate::HtmlSaverHandle).

use std::time::Duration;

use tokio::sync::{mpsc, oneshot};
use tokio::time::{self, MissedTickBehavior};

use crate::sanitizer::SanitizerPipeline;
use crate::saveable::Saveable;
use crate::storage::Storage;

pub async fn run<S: Storage, R: Saveable>(
    mut rx: mpsc::Receiver<R>,
    mut shutdown_rx: oneshot::Receiver<()>,
    storage: S,
    sanitizers: SanitizerPipeline,
    prefix: String,
    batch_size: usize,
    flush_interval: Duration,
) {
    let mut batch: Vec<R> = Vec::with_capacity(batch_size);
    let mut interval = time::interval(flush_interval);
    interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
    // Skip the first immediate tick
    interval.tick().await;

    loop {
        tokio::select! {
            biased;

            _ = &mut shutdown_rx => {
                tracing::info!("Shutdown signal received, draining channel");
                // Drain remaining items
                rx.close();
                while let Some(item) = rx.recv().await {
                    batch.push(item);
                }
                if !batch.is_empty() {
                    flush_batch(&storage, &sanitizers, &prefix, &mut batch).await;
                }
                tracing::info!("Worker shut down");
                return;
            }

            Some(item) = rx.recv() => {
                batch.push(item);
                if batch.len() >= batch_size {
                    flush_batch(&storage, &sanitizers, &prefix, &mut batch).await;
                }
            }

            _ = interval.tick() => {
                if !batch.is_empty() {
                    flush_batch(&storage, &sanitizers, &prefix, &mut batch).await;
                }
            }
        }
    }
}

async fn flush_batch<S: Storage, R: Saveable>(
    storage: &S,
    sanitizers: &SanitizerPipeline,
    prefix: &str,
    batch: &mut Vec<R>,
) {
    let items: Vec<R> = std::mem::take(batch);
    let count = items.len();
    tracing::debug!("Flushing batch of {count} items");

    let futs = items.iter().map(|item| {
        let content = if sanitizers.is_empty() {
            item.content().to_string()
        } else {
            sanitizers.sanitize(item.content())
        };

        let key = if prefix.is_empty() {
            item.name()
        } else {
            format!("{}/{}", prefix, item.name())
        };

        let storage = &storage;
        async move {
            if let Err(e) = storage.put(&key, content.as_bytes(), "text/html").await {
                tracing::error!("Failed to upload {key}: {e}");
            }
        }
    });

    futures::future::join_all(futs).await;
    tracing::debug!("Flushed {count} items");
}
