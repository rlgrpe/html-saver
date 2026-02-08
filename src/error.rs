//! Error types for the `html_saver` crate.

/// All errors that can occur during HTML saving operations.
#[derive(Debug, thiserror::Error)]
pub enum HtmlSaverError {
    /// A storage backend failed to persist content.
    #[error("Storage upload failed: {0}")]
    StorageUpload(Box<dyn std::error::Error + Send + Sync>),

    /// The internal channel to the background worker is closed or full.
    #[error("Channel closed or full")]
    ChannelClosed,

    /// A sanitizer encountered an error while processing HTML.
    #[error("Sanitizer error: {0}")]
    Sanitizer(String),

    /// The builder configuration is invalid.
    #[error("Config error: {0}")]
    Config(String),
}

/// A type alias for `Result<T, HtmlSaverError>`.
pub type Result<T> = std::result::Result<T, HtmlSaverError>;
