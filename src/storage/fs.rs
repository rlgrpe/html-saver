//! Filesystem storage backend.

use std::path::PathBuf;

use crate::error::{HtmlSaverError, Result};
use crate::storage::Storage;

/// Storage backend that writes files to the local filesystem.
///
/// Intermediate directories are created automatically. The `key` provided to
/// [`Storage::put`] is joined with the base directory to form the final path.
///
/// # Example
///
/// ```rust,no_run
/// use html_saver::FsStorage;
///
/// let storage = FsStorage::new("/var/data/html_snapshots");
/// ```
pub struct FsStorage {
    base_dir: PathBuf,
}

impl FsStorage {
    /// Create a new `FsStorage` rooted at the given directory.
    pub fn new(base_dir: impl Into<PathBuf>) -> Self {
        Self {
            base_dir: base_dir.into(),
        }
    }
}

impl Storage for FsStorage {
    async fn put(&self, key: &str, content: &[u8], _content_type: &str) -> Result<()> {
        let path = self.base_dir.join(key);

        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| HtmlSaverError::StorageUpload(Box::new(e)))?;
        }

        tokio::fs::write(&path, content)
            .await
            .map_err(|e| HtmlSaverError::StorageUpload(Box::new(e)))?;

        tracing::debug!("Wrote {} bytes to {}", content.len(), path.display());
        Ok(())
    }
}
