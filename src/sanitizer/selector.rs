//! CSS-selector-based HTML sanitizer.

use scraper::{Html, Selector, node::Node};

use super::Sanitizer;

/// Action to perform on HTML elements matching a CSS selector.
#[derive(Clone, Debug)]
pub enum SelectorAction {
    /// Remove the specified attribute from matching elements.
    RemoveAttr(String),
    /// Remove the entire matching element (and its children) from the document.
    RemoveElement,
    /// Replace the text content of matching elements with the given string.
    ReplaceText(String),
}

/// Sanitizer that uses CSS selectors to locate and modify HTML elements.
///
/// Each rule is a `(css_selector, action)` pair. Rules are applied in order
/// and each rule re-parses the HTML to account for changes made by earlier rules.
///
/// # Example
///
/// ```
/// use html_saver::{SelectorSanitizer, SelectorAction, Sanitizer};
///
/// let sanitizer = SelectorSanitizer::new(vec![
///     ("script", SelectorAction::RemoveElement),
///     (".secret", SelectorAction::ReplaceText("[REDACTED]".into())),
/// ]);
/// let html = r#"<div class="secret">top-secret</div><script>alert(1)</script>"#;
/// let result = sanitizer.sanitize(html);
/// assert!(!result.contains("alert"));
/// assert!(result.contains("[REDACTED]"));
/// ```
pub struct SelectorSanitizer {
    rules: Vec<(String, SelectorAction)>,
}

impl SelectorSanitizer {
    /// Create a new `SelectorSanitizer` from `(css_selector, action)` pairs.
    pub fn new(rules: Vec<(&str, SelectorAction)>) -> Self {
        Self {
            rules: rules
                .into_iter()
                .map(|(sel, action)| (sel.to_string(), action))
                .collect(),
        }
    }
}

/// HTML5 void elements that must not have a closing tag.
const VOID_ELEMENTS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

/// Serialize an HTML tree back to string, skipping nodes in `skip_ids` and
/// applying attribute/text replacements from `replace_attrs`/`replace_texts`.
fn serialize_tree(
    html: &Html,
    skip_ids: &std::collections::HashSet<ego_tree::NodeId>,
    remove_attrs: &std::collections::HashMap<ego_tree::NodeId, String>,
    replace_texts: &std::collections::HashMap<ego_tree::NodeId, String>,
) -> String {
    let mut out = String::new();
    serialize_node(
        html.tree.root(),
        skip_ids,
        remove_attrs,
        replace_texts,
        &mut out,
    );
    out
}

fn serialize_node(
    node: ego_tree::NodeRef<Node>,
    skip_ids: &std::collections::HashSet<ego_tree::NodeId>,
    remove_attrs: &std::collections::HashMap<ego_tree::NodeId, String>,
    replace_texts: &std::collections::HashMap<ego_tree::NodeId, String>,
    out: &mut String,
) {
    let id = node.id();
    if skip_ids.contains(&id) {
        return;
    }

    match node.value() {
        Node::Document | Node::Fragment => {
            for child in node.children() {
                serialize_node(child, skip_ids, remove_attrs, replace_texts, out);
            }
        }
        Node::Element(el) => {
            let tag = el.name();
            out.push('<');
            out.push_str(tag);

            let attr_to_remove = remove_attrs.get(&id);
            for (k, v) in el.attrs() {
                if attr_to_remove.is_some_and(|a| a == k) {
                    continue;
                }
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                out.push_str(v);
                out.push('"');
            }
            out.push('>');

            if VOID_ELEMENTS.contains(&tag) {
                return;
            }

            if let Some(replacement) = replace_texts.get(&id) {
                out.push_str(replacement);
            } else {
                for child in node.children() {
                    serialize_node(child, skip_ids, remove_attrs, replace_texts, out);
                }
            }

            out.push_str("</");
            out.push_str(tag);
            out.push('>');
        }
        Node::Text(text) => {
            out.push_str(text.as_ref());
        }
        Node::Comment(comment) => {
            out.push_str("<!--");
            out.push_str(comment.as_ref());
            out.push_str("-->");
        }
        _ => {}
    }
}

impl Sanitizer for SelectorSanitizer {
    fn sanitize(&self, html: &str) -> String {
        let mut result = html.to_string();

        for (selector_str, action) in &self.rules {
            let Ok(selector) = Selector::parse(selector_str) else {
                tracing::warn!("Invalid CSS selector: {selector_str}");
                continue;
            };

            let document = Html::parse_fragment(&result);

            let mut skip_ids = std::collections::HashSet::new();
            let mut remove_attrs = std::collections::HashMap::new();
            let mut replace_texts = std::collections::HashMap::new();

            for element in document.select(&selector) {
                let node_id = element.id();
                match action {
                    SelectorAction::RemoveElement => {
                        skip_ids.insert(node_id);
                    }
                    SelectorAction::RemoveAttr(attr) => {
                        remove_attrs.insert(node_id, attr.clone());
                    }
                    SelectorAction::ReplaceText(text) => {
                        replace_texts.insert(node_id, text.clone());
                    }
                }
            }

            result = serialize_tree(&document, &skip_ids, &remove_attrs, &replace_texts);
        }

        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remove_script_tags() {
        let sanitizer = SelectorSanitizer::new(vec![("script", SelectorAction::RemoveElement)]);
        let html = r#"<html><head><script>alert('xss')</script></head><body><p>Hello</p><script src="tracker.js"></script></body></html>"#;
        let result = sanitizer.sanitize(html);
        assert!(!result.contains("<script"));
        assert!(!result.contains("alert"));
        assert!(!result.contains("tracker.js"));
        assert!(result.contains("<p>Hello</p>"));
    }

    #[test]
    fn remove_hidden_inputs() {
        let sanitizer = SelectorSanitizer::new(vec![(
            r#"input[type="hidden"]"#,
            SelectorAction::RemoveElement,
        )]);
        let html = r#"<form><input type="hidden" name="csrf" value="token123"><input type="text" name="user"></form>"#;
        let result = sanitizer.sanitize(html);
        assert!(!result.contains("csrf"));
        assert!(!result.contains("token123"));
        assert!(result.contains(r#"type="text""#));
    }

    #[test]
    fn remove_tracking_pixels() {
        let sanitizer = SelectorSanitizer::new(vec![(
            r#"img[width="1"][height="1"]"#,
            SelectorAction::RemoveElement,
        )]);
        let html = r#"<img src="photo.jpg" width="640" height="480"><img src="track.gif" width="1" height="1">"#;
        let result = sanitizer.sanitize(html);
        assert!(!result.contains("track.gif"));
        assert!(result.contains("photo.jpg"));
    }

    #[test]
    fn remove_attribute_from_elements() {
        let sanitizer = SelectorSanitizer::new(vec![(
            "a",
            SelectorAction::RemoveAttr("onclick".to_string()),
        )]);
        let html = r#"<a href="/page" onclick="track()">Link</a>"#;
        let result = sanitizer.sanitize(html);
        assert!(result.contains(r#"href="/page""#));
        assert!(!result.contains("onclick"));
        assert!(result.contains("Link"));
    }

    #[test]
    fn replace_text_content() {
        let sanitizer = SelectorSanitizer::new(vec![(
            ".secret",
            SelectorAction::ReplaceText("[REDACTED]".to_string()),
        )]);
        let html =
            r#"<span class="secret">my-api-key-12345</span><span class="public">visible</span>"#;
        let result = sanitizer.sanitize(html);
        assert!(result.contains("[REDACTED]"));
        assert!(!result.contains("my-api-key-12345"));
        assert!(result.contains("visible"));
    }

    #[test]
    fn remove_noscript_and_style_elements() {
        let sanitizer = SelectorSanitizer::new(vec![
            ("noscript", SelectorAction::RemoveElement),
            ("style", SelectorAction::RemoveElement),
        ]);
        let html = r#"<html><head><style>body{color:red}</style></head><body><p>Content</p><noscript>Enable JS</noscript></body></html>"#;
        let result = sanitizer.sanitize(html);
        assert!(!result.contains("<style"));
        assert!(!result.contains("<noscript"));
        assert!(result.contains("Content"));
    }

    #[test]
    fn invalid_selector_is_skipped() {
        let sanitizer = SelectorSanitizer::new(vec![("[[[invalid", SelectorAction::RemoveElement)]);
        let html = "<p>unchanged</p>";
        let result = sanitizer.sanitize(html);
        assert!(result.contains("unchanged"));
    }
}
