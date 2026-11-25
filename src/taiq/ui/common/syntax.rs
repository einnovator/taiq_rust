//! Small shared lexical highlighter for source previews.
use regex::Regex;
use std::ops::Range;
use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub enum TokenKind {
    Keyword,
    String,
    Comment,
    Number,
}

pub fn tokens(source: &str) -> Vec<(Range<usize>, TokenKind)> {
    let pattern = r#"(?m)//[^\n]*|#[^\n]*|"(?:\\.|[^"\\])*"|\b(?:fn|let|mut|pub|impl|struct|enum|use|mod|match|if|else|for|while|return|async|await|def|class|import|from|const|var|function|export|interface|type|true|false|null|None|Some|Ok|Err)\b|\b\d+(?:\.\d+)?\b"#;
    static TOKEN_RE: OnceLock<Regex> = OnceLock::new();
    let re = TOKEN_RE.get_or_init(|| Regex::new(pattern).expect("valid syntax token regex"));
    re.find_iter(source)
        .map(|token| {
            let word = token.as_str();
            let kind = if word.starts_with("//") || word.starts_with('#') {
                TokenKind::Comment
            } else if word.starts_with('"') {
                TokenKind::String
            } else if word.as_bytes()[0].is_ascii_digit() {
                TokenKind::Number
            } else {
                TokenKind::Keyword
            };
            (token.range(), kind)
        })
        .collect()
}
