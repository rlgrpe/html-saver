//! Pluggable storage backends for persisting HTML content.
//!
//! The crate ships with two built-in backends:
//!
//! - [`FsStorage`] -- writes to the local filesystem.
//! - [`S3Storage`] -- writes to an Amazon S3 (or compatible) bucket
//!   (requires the `s3` feature).
//!
//! Implement the [`Storage`] trait to add your own backend.

mod fs;
#[cfg(feature = "s3")]
mod s3;

#[cfg(feature = "s3")]
pub use aws_config::Region;
#[cfg(feature = "s3")]
pub use aws_sdk_s3::config::Credentials;
#[cfg(feature = "s3")]
pub use aws_sdk_s3::{Client as S3Client, Config as S3Config, config::Builder as S3ConfigBuilder};
pub use fs::FsStorage;
#[cfg(feature = "s3")]
pub use s3::S3Storage;

use crate::error::Result;

use std::future::Future;

/// Trait for storage backends that can persist HTML content.
///
/// Implementations must be `Send + Sync + 'static` so they can be used from
/// the background worker task.
///
/// # Implementing a custom backend
///
/// ```rust,no_run
/// use html_saver::{Storage, Result};
///
/// struct MyStorage;
///
/// impl Storage for MyStorage {
///     async fn put(&self, key: &str, content: &[u8], content_type: &str) -> Result<()> {
///         // write content somewhere ...
///         Ok(())
///     }
/// }
/// ```
pub trait Storage: Send + Sync + 'static {
    /// Persist `content` under the given `key` with the specified MIME
    /// `content_type` (typically `"text/html"`).
    fn put(
        &self,
        key: &str,
        content: &[u8],
        content_type: &str,
    ) -> impl Future<Output = Result<()>> + Send;
}
