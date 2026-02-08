//! Handles for submitting save requests and controlling the background worker.

use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

use crate::error::{HtmlSaverError, Result};
use crate::saveable::Saveable;

/// Primary handle returned by [`HtmlSaverBuilder::build`](crate::HtmlSaverBuilder::build).
///
/// Owns the shutdown signal and the worker task join handle. Use [`save`](Self::save)
/// to queue items and [`shutdown`](Self::shutdown) to gracefully stop the worker
/// and flush any remaining batched items.
///
/// For sharing across multiple tasks, obtain a lightweight [`HtmlSaverSender`]
/// via [`sender`](Self::sender).
pub struct HtmlSaverHandle<R: Saveable> {
    sender: mpsc::Sender<R>,
    shutdown: Option<oneshot::Sender<()>>,
    worker: Option<JoinHandle<()>>,
}

impl<R: Saveable> HtmlSaverHandle<R> {
    pub(crate) fn new(
        sender: mpsc::Sender<R>,
        shutdown: oneshot::Sender<()>,
        worker: JoinHandle<()>,
    ) -> Self {
        Self {
            sender,
            shutdown: Some(shutdown),
            worker: Some(worker),
        }
    }

    /// Queue an item for saving.
    ///
    /// This is a non-blocking operation that places the item into the internal
    /// channel. Returns [`HtmlSaverError::ChannelClosed`] if the channel is
    /// full or the worker has stopped.
    pub fn save(&self, request: R) -> Result<()> {
        self.sender
            .try_send(request)
            .map_err(|_| HtmlSaverError::ChannelClosed)
    }

    /// Queue an item for saving, logging the error via `tracing` on failure
    /// instead of returning it.
    pub fn save_or_log(&self, request: R) {
        if let Err(e) = self.save(request) {
            tracing::error!("Failed to queue save request: {e}");
        }
    }

    /// Create a lightweight, cloneable [`HtmlSaverSender`] that shares the
    /// same underlying channel.
    pub fn sender(&self) -> HtmlSaverSender<R> {
        HtmlSaverSender {
            sender: self.sender.clone(),
        }
    }

    /// Gracefully shut down the background worker.
    ///
    /// Sends a shutdown signal, waits for the worker to drain any remaining
    /// items in the channel and flush the final batch, then returns.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        if let Some(handle) = self.worker.take() {
            let _ = handle.await;
        }
    }
}

/// Lightweight, cloneable sender for submitting save requests from multiple tasks.
///
/// Obtained via [`HtmlSaverHandle::sender`]. Does **not** own the shutdown
/// signal or the worker join handle -- dropping all senders will not stop the
/// worker.
pub struct HtmlSaverSender<R: Saveable> {
    sender: mpsc::Sender<R>,
}

impl<R: Saveable> Clone for HtmlSaverSender<R> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<R: Saveable> HtmlSaverSender<R> {
    /// Queue an item for saving.
    pub fn save(&self, request: R) -> Result<()> {
        self.sender
            .try_send(request)
            .map_err(|_| HtmlSaverError::ChannelClosed)
    }

    /// Queue an item for saving, logging errors instead of returning them.
    pub fn save_or_log(&self, request: R) {
        if let Err(e) = self.save(request) {
            tracing::error!("Failed to queue save request: {e}");
        }
    }
}
