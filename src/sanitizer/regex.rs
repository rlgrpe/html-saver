//! Regex-based HTML sanitizer.

use regex::Regex;

use super::Sanitizer;

/// Sanitizer that applies a series of regex find-and-replace rules.
///
/// Rules are applied in order; each rule operates on the output of the
/// previous one.
///
/// # Example
///
/// ```
/// use html_saver::RegexSanitizer;
/// use html_saver::Sanitizer;
///
/// let sanitizer = RegexSanitizer::new(vec![
///     (r"\d{4}-\d{4}-\d{4}-\d{4}", "[CARD REDACTED]"),
/// ]);
/// let result = sanitizer.sanitize("Card: 4111-1111-1111-1111");
/// assert!(result.contains("[CARD REDACTED]"));
/// ```
pub struct RegexSanitizer {
    rules: Vec<(Regex, String)>,
}

impl RegexSanitizer {
    /// Create a new `RegexSanitizer` from a list of `(pattern, replacement)` pairs.
    ///
    /// # Panics
    ///
    /// Panics if any regex pattern is invalid. Use [`try_new`](Self::try_new)
    /// for a fallible alternative.
    pub fn new(rules: Vec<(&str, &str)>) -> Self {
        let rules = rules
            .into_iter()
            .map(|(pattern, replacement)| {
                (
                    Regex::new(pattern).expect("invalid regex pattern"),
                    replacement.to_string(),
                )
            })
            .collect();
        Self { rules }
    }

    /// Fallible constructor that returns a [`regex::Error`] for invalid patterns.
    pub fn try_new(rules: Vec<(&str, &str)>) -> Result<Self, regex::Error> {
        let rules = rules
            .into_iter()
            .map(|(pattern, replacement)| Ok((Regex::new(pattern)?, replacement.to_string())))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self { rules })
    }
}

impl Sanitizer for RegexSanitizer {
    fn sanitize(&self, html: &str) -> String {
        self.rules
            .iter()
            .fold(html.to_string(), |acc, (re, replacement)| {
                re.replace_all(&acc, replacement.as_str()).into_owned()
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_phone_numbers() {
        let sanitizer = RegexSanitizer::new(vec![(r"\+?\d[\d\-\s]{8,}\d", "[PHONE REDACTED]")]);
        let html = r#"<p>Call us at +1-800-555-1234 or +44 20 7946 0958</p>"#;
        let result = sanitizer.sanitize(html);
        assert!(!result.contains("800-555-1234"));
        assert!(!result.contains("7946 0958"));
        assert!(result.contains("[PHONE REDACTED]"));
    }

    #[test]
    fn remove_credit_card_patterns() {
        let sanitizer = RegexSanitizer::new(vec![(
            r"\b\d{4}[\s\-]?\d{4}[\s\-]?\d{4}[\s\-]?\d{4}\b",
            "[CARD REDACTED]",
        )]);
        let html = r#"<span>Card: 4111-1111-1111-1111</span><span>Also 5500 0000 0000 0004</span>"#;
        let result = sanitizer.sanitize(html);
        assert_eq!(result.matches("[CARD REDACTED]").count(), 2);
        assert!(!result.contains("4111"));
        assert!(!result.contains("5500"));
    }

    #[test]
    fn remove_email_addresses() {
        let sanitizer = RegexSanitizer::new(vec![(
            r"[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}",
            "[EMAIL REDACTED]",
        )]);
        let html =
            r#"<a href="mailto:user@example.com">user@example.com</a> and admin+test@corp.co.uk"#;
        let result = sanitizer.sanitize(html);
        assert!(!result.contains("user@example.com"));
        assert!(!result.contains("admin+test@corp.co.uk"));
        assert_eq!(result.matches("[EMAIL REDACTED]").count(), 3); // href + text + second
    }

    #[test]
    fn multiple_rules_applied_in_order() {
        let sanitizer = RegexSanitizer::new(vec![
            (r"\d{3}-\d{2}-\d{4}", "[SSN]"), // SSN pattern
            (r"\[SSN\]", "***-**-****"),     // then mask the placeholder
        ]);
        let result = sanitizer.sanitize("SSN: 123-45-6789");
        assert_eq!(result, "SSN: ***-**-****");
    }

    #[test]
    fn no_rules_returns_original() {
        let sanitizer = RegexSanitizer::new(vec![]);
        let html = "<p>unchanged</p>";
        assert_eq!(sanitizer.sanitize(html), html);
    }

    #[test]
    fn try_new_invalid_pattern() {
        let result = RegexSanitizer::try_new(vec![("[invalid", "x")]);
        assert!(result.is_err());
    }

    #[test]
    fn try_new_valid_pattern() {
        let result = RegexSanitizer::try_new(vec![(r"\d+", "NUM")]);
        assert!(result.is_ok());
        let sanitizer = result.unwrap();
        assert_eq!(sanitizer.sanitize("abc 123 def"), "abc NUM def");
    }
}
