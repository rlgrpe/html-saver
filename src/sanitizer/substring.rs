//! Simple literal-string-replacement sanitizer.

use super::Sanitizer;

/// Sanitizer that performs exact substring replacements.
///
/// Rules are applied in order; each rule operates on the output of the
/// previous one.
///
/// # Example
///
/// ```
/// use html_saver::{SubstringSanitizer, Sanitizer};
///
/// let s = SubstringSanitizer::new(vec![("password123", "***")]);
/// assert_eq!(s.sanitize("pw=password123"), "pw=***");
/// ```
pub struct SubstringSanitizer {
    rules: Vec<(String, String)>,
}

impl SubstringSanitizer {
    /// Create a new `SubstringSanitizer` from `(needle, replacement)` pairs.
    pub fn new(rules: Vec<(&str, &str)>) -> Self {
        Self {
            rules: rules
                .into_iter()
                .map(|(needle, replacement)| (needle.to_string(), replacement.to_string()))
                .collect(),
        }
    }
}

impl Sanitizer for SubstringSanitizer {
    fn sanitize(&self, html: &str) -> String {
        self.rules
            .iter()
            .fold(html.to_string(), |acc, (needle, replacement)| {
                acc.replace(needle, replacement)
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replace_api_key_placeholder() {
        let sanitizer = SubstringSanitizer::new(vec![
            ("sk-live-abc123xyz", "[API_KEY_REDACTED]"),
            ("Bearer token-secret-456", "Bearer [TOKEN_REDACTED]"),
        ]);
        let html = r#"<div data-key="sk-live-abc123xyz">Bearer token-secret-456</div>"#;
        let result = sanitizer.sanitize(html);
        assert_eq!(
            result,
            r#"<div data-key="[API_KEY_REDACTED]">Bearer [TOKEN_REDACTED]</div>"#
        );
    }

    #[test]
    fn replace_sensitive_tokens() {
        let sanitizer = SubstringSanitizer::new(vec![
            ("CSRF_TOKEN_VALUE_HERE", "***"),
            ("session_id=abc123", "session_id=REDACTED"),
        ]);
        let html = r#"<input type="hidden" value="CSRF_TOKEN_VALUE_HERE"><meta content="session_id=abc123">"#;
        let result = sanitizer.sanitize(html);
        assert!(!result.contains("CSRF_TOKEN_VALUE_HERE"));
        assert!(!result.contains("session_id=abc123"));
        assert!(result.contains("***"));
        assert!(result.contains("session_id=REDACTED"));
    }

    #[test]
    fn multiple_occurrences_all_replaced() {
        let sanitizer = SubstringSanitizer::new(vec![("SECRET", "***")]);
        let html = "SECRET and SECRET and SECRET";
        let result = sanitizer.sanitize(html);
        assert_eq!(result, "*** and *** and ***");
    }

    #[test]
    fn no_match_returns_original() {
        let sanitizer = SubstringSanitizer::new(vec![("nothere", "replaced")]);
        let html = "<p>original content</p>";
        assert_eq!(sanitizer.sanitize(html), html);
    }

    #[test]
    fn empty_rules() {
        let sanitizer = SubstringSanitizer::new(vec![]);
        let html = "<p>unchanged</p>";
        assert_eq!(sanitizer.sanitize(html), html);
    }

    #[test]
    fn rules_applied_sequentially() {
        let sanitizer = SubstringSanitizer::new(vec![
            ("AAA", "BBB"),
            ("BBB", "CCC"), // Should also replace the BBB from the first rule
        ]);
        let result = sanitizer.sanitize("AAA");
        assert_eq!(result, "CCC");
    }
}
