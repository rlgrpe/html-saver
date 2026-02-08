//! Builder for configuring and launching the background HTML-saving worker.

use std::time::Duration;

use crate::handle::HtmlSaverHandle;
use crate::sanitizer::{Sanitizer, SanitizerPipeline};
use crate::saveable::Saveable;
use crate::storage::Storage;
use crate::worker;

/// Builder for configuring and starting an [`HtmlSaverHandle`].
///
/// Provides a fluent API for setting batch size, flush interval, channel
/// buffer capacity, storage key prefix, and the sanitizer pipeline.
///
/// # Example
///
/// ```rust,no_run
/// use html_saver::{HtmlSaverBuilder, FsStorage, Saveable, SubstringSanitizer};
/// use std::time::Duration;
///
/// # struct MyRequest;
/// # impl Saveable for MyRequest {
/// #     fn content(&self) -> &str { "" }
/// #     fn name(&self) -> String { String::new() }
/// # }
/// # async fn example() {
/// let handle = HtmlSaverBuilder::new(FsStorage::new("/tmp/html"))
///     .batch_size(100)
///     .flush_interval(Duration::from_secs(10))
///     .channel_buffer(5000)
///     .prefix("snapshots/v1")
///     .add_sanitizer(SubstringSanitizer::new(vec![("secret", "***")]))
///     .build::<MyRequest>();
/// # }
/// ```
pub struct HtmlSaverBuilder<S: Storage> {
    storage: S,
    batch_size: usize,
    flush_interval: Duration,
    channel_buffer: usize,
    sanitizers: SanitizerPipeline,
    prefix: String,
}

impl<S: Storage> HtmlSaverBuilder<S> {
    /// Create a new builder with the given storage backend and sensible defaults.
    ///
    /// Defaults: batch size 50, flush interval 5 s, channel buffer 1000,
    /// no sanitizers, no prefix.
    pub fn new(storage: S) -> Self {
        Self {
            storage,
            batch_size: 50,
            flush_interval: Duration::from_secs(5),
            channel_buffer: 1000,
            sanitizers: SanitizerPipeline::new(),
            prefix: String::new(),
        }
    }

    /// Maximum number of items to batch before flushing to storage.
    pub fn batch_size(mut self, size: usize) -> Self {
        self.batch_size = size;
        self
    }

    /// Time interval after which the batch is flushed regardless of size.
    pub fn flush_interval(mut self, interval: Duration) -> Self {
        self.flush_interval = interval;
        self
    }

    /// Capacity of the internal mpsc channel between producers and the worker.
    pub fn channel_buffer(mut self, size: usize) -> Self {
        self.channel_buffer = size;
        self
    }

    /// Append a [`Sanitizer`] to the processing pipeline.
    ///
    /// Sanitizers run in the order they are added, each receiving the output
    /// of the previous one.
    pub fn add_sanitizer(mut self, sanitizer: impl Sanitizer + 'static) -> Self {
        self.sanitizers.add(sanitizer);
        self
    }

    /// Set a prefix that is prepended to every storage key (separated by `/`).
    pub fn prefix(mut self, prefix: impl Into<String>) -> Self {
        self.prefix = prefix.into();
        self
    }

    /// Consume the builder, spawn the background worker, and return the
    /// [`HtmlSaverHandle`] used to submit items and control the worker lifecycle.
    pub fn build<R: Saveable>(self) -> HtmlSaverHandle<R> {
        let (tx, rx) = tokio::sync::mpsc::channel::<R>(self.channel_buffer);
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let worker_handle = tokio::spawn(worker::run(
            rx,
            shutdown_rx,
            self.storage,
            self.sanitizers,
            self.prefix,
            self.batch_size,
            self.flush_interval,
        ));

        HtmlSaverHandle::new(tx, shutdown_tx, worker_handle)
    }
}
