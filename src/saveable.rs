//! The [`Saveable`] trait that user types implement to provide HTML content
//! and naming information for persistence.

/// Trait implemented by user-defined request types that carry HTML content
/// to be saved.
///
/// The struct implementing this trait holds all metadata required to produce
/// the storage key ([`name`](Saveable::name)) and the HTML body
/// ([`content`](Saveable::content)).
///
/// # Example
///
/// ```
/// use html_saver::Saveable;
///
/// struct PageSnapshot {
///     url: String,
///     html: String,
/// }
///
/// impl Saveable for PageSnapshot {
///     fn content(&self) -> &str {
///         &self.html
///     }
///
///     fn name(&self) -> String {
///         format!("{}.html", self.url.replace('/', "_"))
///     }
/// }
/// ```
pub trait Saveable: Send + 'static {
    /// Returns the raw HTML content to save.
    ///
    /// This content will be passed through the sanitizer pipeline (if any)
    /// before being written to storage.
    fn content(&self) -> &str;

    /// Generates the storage key (file path / object key) for this item.
    ///
    /// Called by the background worker at flush time. If a prefix is
    /// configured on the builder, it will be prepended automatically.
    fn name(&self) -> String;
}
