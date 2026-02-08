//! Amazon S3 storage backend (requires the `s3` feature).

use aws_sdk_s3::Client;

use crate::error::{HtmlSaverError, Result};
use crate::storage::Storage;

/// Storage backend that uploads files to an Amazon S3 (or S3-compatible) bucket.
///
/// # Example
///
/// ```rust,ignore
/// use html_saver::{S3Storage, Credentials, Region, S3Config};
///
/// let creds = Credentials::new("AKID", "SECRET", None, None, "my-app");
/// let config = S3Config::builder()
///     .region(Region::new("us-east-1"))
///     .credentials_provider(creds)
///     .build();
/// let storage = S3Storage::from_conf(config, "my-bucket");
/// ```
pub struct S3Storage {
    client: Client,
    bucket: String,
}

impl S3Storage {
    /// Create a new `S3Storage` with an existing [`Client`] and bucket name.
    pub fn new(client: Client, bucket: impl Into<String>) -> Self {
        Self {
            client,
            bucket: bucket.into(),
        }
    }

    /// Create an `S3Storage` from an [`aws_sdk_s3::Config`].
    ///
    /// ```ignore
    /// let credentials = Credentials::new(&key, &secret, None, None, "my-app");
    /// let config = S3Config::builder()
    ///     .region(Region::new("us-east-1"))
    ///     .endpoint_url("https://s3.example.com")
    ///     .credentials_provider(credentials)
    ///     .force_path_style(true)
    ///     .build();
    /// let storage = S3Storage::from_conf(config, "my-bucket");
    /// ```
    pub fn from_conf(config: aws_sdk_s3::Config, bucket: impl Into<String>) -> Self {
        let client = Client::from_conf(config);
        Self::new(client, bucket)
    }

    /// Create an `S3Storage` using credentials and region from the AWS
    /// environment (env vars, config files, IMDS, etc.).
    pub async fn from_env(bucket: impl Into<String>) -> Self {
        let config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .load()
            .await;
        let client = Client::new(&config);
        Self::new(client, bucket)
    }
}

impl Storage for S3Storage {
    async fn put(&self, key: &str, content: &[u8], content_type: &str) -> Result<()> {
        self.client
            .put_object()
            .bucket(&self.bucket)
            .key(key)
            .body(content.to_vec().into())
            .content_type(content_type)
            .send()
            .await
            .map_err(|e| HtmlSaverError::StorageUpload(Box::new(e)))?;

        tracing::debug!(
            "Uploaded {} bytes to s3://{}/{}",
            content.len(),
            self.bucket,
            key
        );
        Ok(())
    }
}
