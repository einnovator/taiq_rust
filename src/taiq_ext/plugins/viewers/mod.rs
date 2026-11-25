mod markdown;
mod structured;
mod text;
mod tiptap;

use super::PluginDescriptor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreviewBlock {
    Heading(u8, String),
    Paragraph(String),
    Code(String),
    ListItem(String),
    Quote(String),
    Rule,
}

pub fn markdown_preview(source: &str) -> Vec<PreviewBlock> {
    markdown::parse(source)
}
pub fn tiptap_preview(source: &str) -> Option<Vec<PreviewBlock>> {
    tiptap::preview(source)
}
pub fn document_preview(path: &str, source: &str) -> Option<Vec<PreviewBlock>> {
    if path.to_ascii_lowercase().ends_with(".md") || path.to_ascii_lowercase().ends_with(".mdx") {
        return Some(markdown_preview(source));
    }
    if [
        "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "c", "cpp", "h", "toml", "yaml", "yml",
        "sh", "ps1",
    ]
    .iter()
    .any(|ext| path.to_ascii_lowercase().ends_with(&format!(".{ext}")))
    {
        return Some(vec![PreviewBlock::Code(
            source.chars().take(40_000).collect(),
        )]);
    }
    structured::preview(path, source)
}
pub fn all() -> Vec<PluginDescriptor> {
    vec![markdown::descriptor(), text::descriptor()]
}
