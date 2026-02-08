//! # html_saver
//!
//! A batched, async HTML persistence library with pluggable storage backends
//! and a sanitizer pipeline for redacting sensitive content before saving.
//!
//! ## Overview
//!
//! `html_saver` runs a background worker that collects [`Saveable`] items,
//! optionally sanitizes their HTML content through a [`SanitizerPipeline`],
//! and writes the results to a [`Storage`] backend (local filesystem, S3, or
//! your own implementation).
//!
//! Items are batched by count and/or time interval to reduce I/O overhead.
//!
//! ## Quick start
//!
//! ```rust,no_run
//! use html_saver::{HtmlSaverBuilder, FsStorage, Saveable, SubstringSanitizer};
//!
//! struct Page { url: String, body: String }
//!
//! impl Saveable for Page {
//!     fn content(&self) -> &str { &self.body }
//!     fn name(&self) -> String { format!("{}.html", self.url) }
//! }
//!
//! # async fn example() {
//! let handle = HtmlSaverBuilder::new(FsStorage::new("/tmp/pages"))
//!     .batch_size(20)
//!     .add_sanitizer(SubstringSanitizer::new(vec![("secret", "***")]))
//!     .build::<Page>();
//!
//! handle.save(Page { url: "index".into(), body: "<h1>Hi</h1>".into() }).unwrap();
//!
//! // On shutdown, flush remaining items:
//! handle.shutdown().await;
//! # }
//! ```
//!
//! ## Feature flags
//!
//! | Feature | Default | Description |
//! |---------|---------|-------------|
//! | `s3` | **yes** | Enables [`S3Storage`] and re-exports from `aws-sdk-s3` / `aws-config`. |
//! | `rustls-tls` | no | Use `rustls` instead of the platform TLS for the AWS SDK. |

pub mod config;
pub mod error;
pub mod handle;
pub mod sanitizer;
pub mod saveable;
pub mod storage;
mod worker;

pub use config::HtmlSaverBuilder;
pub use error::{HtmlSaverError, Result};
pub use handle::{HtmlSaverHandle, HtmlSaverSender};
pub use sanitizer::{
    RegexSanitizer, Sanitizer, SanitizerPipeline, SelectorAction, SelectorSanitizer,
    SubstringSanitizer,
};
pub use saveable::Saveable;
#[cfg(feature = "s3")]
pub use storage::{Credentials, Region, S3Client, S3Config, S3ConfigBuilder, S3Storage};
pub use storage::{FsStorage, Storage};

use std::any::Any;
use std::sync::OnceLock;

// Global state for the optional singleton pattern
static GLOBAL: OnceLock<Box<dyn Any + Send + Sync>> = OnceLock::new();

/// Initialize the global [`HtmlSaverHandle`] singleton.
///
/// Call once at application startup. The returned [`HtmlSaverHandle`] must be
/// kept alive for the lifetime of the application and [`HtmlSaverHandle::shutdown`]
/// should be called before exit to flush pending items.
///
/// After calling this, any part of the application can obtain a
/// [`HtmlSaverSender`] via [`global()`].
///
/// # Panics
///
/// Panics if called more than once.
pub fn init<S: Storage, R: Saveable + 'static>(builder: HtmlSaverBuilder<S>) -> HtmlSaverHandle<R> {
    let handle = builder.build::<R>();
    let sender = handle.sender();

    GLOBAL
        .set(Box::new(sender))
        .unwrap_or_else(|_| panic!("Global HtmlSaver already initialized"));

    handle
}

/// Retrieve the global [`HtmlSaverSender`] previously registered with [`init()`].
///
/// Returns `None` if [`init()`] has not been called or if the type parameter `R`
/// does not match the type used during initialization.
pub fn global<R: Saveable + 'static>() -> Option<&'static HtmlSaverSender<R>> {
    GLOBAL
        .get()
        .and_then(|any| any.downcast_ref::<HtmlSaverSender<R>>())
}