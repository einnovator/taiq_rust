//! Inline Markdown spans shared by native chat and document previews.
use regex::Regex;
use std::ops::Range;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InlineStyle {
    Strong,
    Emphasis,
    Code,
    Link(String),
}

#[derive(Debug, Clone)]
pub struct InlineText {
    pub plain: String,
    pub spans: Vec<(Range<usize>, InlineStyle)>,
}

pub fn parse(source: &str) -> InlineText {
    static INLINE_RE: OnceLock<Regex> = OnceLock::new();
    let re = INLINE_RE.get_or_init(|| {
        Regex::new(r#"\*\*([^*]+)\*\*|\*([^*]+)\*|`([^`]+)`|\[([^]]+)\]\((https?://[^)]+)\)"#)
            .unwrap()
    });
    let mut plain = String::new();
    let mut spans = Vec::new();
    let mut cursor = 0;
    for capture in re.captures_iter(source) {
        let entire = capture.get(0).unwrap();
        plain.push_str(&source[cursor..entire.start()]);
        let (value, style) = if let Some(value) = capture.get(1) {
            (value.as_str(), InlineStyle::Strong)
        } else if let Some(value) = capture.get(2) {
            (value.as_str(), InlineStyle::Emphasis)
        } else if let Some(value) = capture.get(3) {
            (value.as_str(), InlineStyle::Code)
        } else {
            (
                capture.get(4).unwrap().as_str(),
                InlineStyle::Link(capture.get(5).unwrap().as_str().to_owned()),
            )
        };
        let start = plain.len();
        plain.push_str(value);
        spans.push((start..plain.len(), style));
        cursor = entire.end();
    }
    plain.push_str(&source[cursor..]);
    InlineText { plain, spans }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_inline_styles_without_leaving_markup() {
        let parsed = parse("A **bold** `code` [link](https://example.com)");
        assert_eq!(parsed.plain, "A bold code link");
        assert_eq!(parsed.spans.len(), 3);
        assert!(
            matches!(&parsed.spans[2].1, InlineStyle::Link(url) if url == "https://example.com")
        );
    }
}
