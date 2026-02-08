//! HTML sanitizer pipeline for redacting or transforming content before saving.
//!
//! Sanitizers implement the [`Sanitizer`] trait and are composed into a
//! [`SanitizerPipeline`] that runs them sequentially.
//!
//! Built-in sanitizers:
//!
//! - [`SubstringSanitizer`] -- literal string replacements.
//! - [`RegexSanitizer`] -- regex-based replacements.
//! - [`SelectorSanitizer`] -- CSS-selector-based element manipulation.

mod regex;
mod selector;
mod substring;

pub use self::regex::RegexSanitizer;
pub use selector::{SelectorAction, SelectorSanitizer};
pub use substring::SubstringSanitizer;

/// Trait for HTML content sanitizers.
///
/// Each sanitizer receives an HTML string and returns a transformed version.
/// Implementations must be `Send + Sync` so they can be shared with the
/// background worker.
pub trait Sanitizer: Send + Sync {
    /// Transform the given HTML content, returning the sanitized result.
    fn sanitize(&self, html: &str) -> String;
}

/// An ordered chain of [`Sanitizer`] implementations applied sequentially.
///
/// Each sanitizer receives the output of the previous one. An empty pipeline
/// is a no-op.
pub struct SanitizerPipeline {
    sanitizers: Vec<Box<dyn Sanitizer>>,
}

impl SanitizerPipeline {
    /// Create an empty pipeline.
    pub fn new() -> Self {
        Self {
            sanitizers: Vec::new(),
        }
    }

    /// Append a sanitizer to the end of the pipeline.
    pub fn add(&mut self, sanitizer: impl Sanitizer + 'static) {
        self.sanitizers.push(Box::new(sanitizer));
    }

    /// Run the full pipeline on the given HTML, returning the final result.
    pub fn sanitize(&self, html: &str) -> String {
        self.sanitizers
            .iter()
            .fold(html.to_string(), |acc, s| s.sanitize(&acc))
    }

    /// Returns `true` if no sanitizers have been added.
    pub fn is_empty(&self) -> bool {
        self.sanitizers.is_empty()
    }
}

impl Default for SanitizerPipeline {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_empty_is_empty() {
        let pipeline = SanitizerPipeline::new();
        assert!(pipeline.is_empty());
    }

    #[test]
    fn pipeline_not_empty_after_add() {
        let mut pipeline = SanitizerPipeline::new();
        pipeline.add(SubstringSanitizer::new(vec![("a", "b")]));
        assert!(!pipeline.is_empty());
    }

    #[test]
    fn pipeline_default_is_empty() {
        let pipeline = SanitizerPipeline::default();
        assert!(pipeline.is_empty());
    }

    #[test]
    fn pipeline_chains_sanitizers_in_order() {
        // Realistic pipeline: remove scripts -> strip tokens -> regex clean emails
        let mut pipeline = SanitizerPipeline::new();

        // Step 1: remove script tags via selector
        pipeline.add(SelectorSanitizer::new(vec![(
            "script",
            SelectorAction::RemoveElement,
        )]));

        // Step 2: replace known sensitive tokens
        pipeline.add(SubstringSanitizer::new(vec![(
            "INTERNAL_API_KEY_XYZ",
            "[REDACTED]",
        )]));

        // Step 3: regex-clean any remaining email addresses
        pipeline.add(RegexSanitizer::new(vec![(
            r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}",
            "[EMAIL]",
        )]));

        let html = concat!(
            r#"<html><head><script>var key="INTERNAL_API_KEY_XYZ";</script></head>"#,
            r#"<body><p>Contact admin@example.com or use INTERNAL_API_KEY_XYZ</p></body></html>"#,
        );
        let result = pipeline.sanitize(html);

        assert!(!result.contains("<script"));
        assert!(!result.contains("INTERNAL_API_KEY_XYZ"));
        assert!(!result.contains("admin@example.com"));
        assert!(result.contains("[REDACTED]"));
        assert!(result.contains("[EMAIL]"));
    }

    #[test]
    fn pipeline_with_no_sanitizers_returns_original() {
        let pipeline = SanitizerPipeline::new();
        let html = "<p>original</p>";
        assert_eq!(pipeline.sanitize(html), html);
    }

    #[test]
    fn pipeline_realistic_scraping_cleanup() {
        let mut pipeline = SanitizerPipeline::new();

        // Remove tracking pixels
        pipeline.add(SelectorSanitizer::new(vec![(
            r#"img[width="1"]"#,
            SelectorAction::RemoveElement,
        )]));

        // Remove hidden form fields
        pipeline.add(SelectorSanitizer::new(vec![(
            r#"input[type="hidden"]"#,
            SelectorAction::RemoveElement,
        )]));

        // Strip phone numbers
        pipeline.add(RegexSanitizer::new(vec![(
            r"\+?\d[\d\-\s]{8,}\d",
            "[PHONE]",
        )]));

        let html = concat!(
            r#"<form><input type="hidden" name="token" value="abc">"#,
            r#"<input type="text" name="q"></form>"#,
            r#"<p>Call +1-800-555-0199</p>"#,
            r#"<img src="track.gif" width="1" height="1">"#,
        );
        let result = pipeline.sanitize(html);
        assert!(!result.contains("token"));
        assert!(!result.contains("track.gif"));
        assert!(!result.contains("800-555"));
        assert!(result.contains("[PHONE]"));
        assert!(result.contains(r#"type="text""#));
    }
}
