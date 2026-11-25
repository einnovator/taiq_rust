use super::PreviewBlock;
use serde_json::Value;

pub fn preview(source: &str) -> Option<Vec<PreviewBlock>> {
    let value: Value = serde_json::from_str(source).ok()?;
    let document = if value.get("type").and_then(Value::as_str) == Some("doc") {
        &value
    } else {
        value.get("content")?
    };
    if document.get("type").and_then(Value::as_str) != Some("doc") {
        return None;
    }
    let mut blocks = Vec::new();
    walk(document, &mut blocks);
    Some(blocks)
}

fn walk(node: &Value, blocks: &mut Vec<PreviewBlock>) {
    let kind = node.get("type").and_then(Value::as_str).unwrap_or("");
    match kind {
        "heading" => {
            let level = node
                .get("attrs")
                .and_then(|a| a.get("level"))
                .and_then(Value::as_u64)
                .unwrap_or(1)
                .clamp(1, 6) as u8;
            blocks.push(PreviewBlock::Heading(level, text(node)));
        }
        "paragraph" => blocks.push(PreviewBlock::Paragraph(text(node))),
        "codeBlock" => blocks.push(PreviewBlock::Code(text(node))),
        "listItem" | "taskItem" => {
            blocks.push(PreviewBlock::Paragraph(format!("• {}", text(node))))
        }
        _ => {
            if let Some(children) = node.get("content").and_then(Value::as_array) {
                for child in children {
                    walk(child, blocks);
                }
            }
        }
    }
}

fn text(node: &Value) -> String {
    if let Some(text) = node.get("text").and_then(Value::as_str) {
        return text.to_owned();
    }
    node.get("content")
        .and_then(Value::as_array)
        .map(|children| children.iter().map(text).collect::<String>())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_tiptap_document_blocks() {
        let input = r#"{"type":"doc","content":[{"type":"heading","attrs":{"level":2},"content":[{"type":"text","text":"Title"}]},{"type":"paragraph","content":[{"type":"text","text":"Hello"}]}]}"#;
        assert_eq!(
            preview(input),
            Some(vec![
                PreviewBlock::Heading(2, "Title".into()),
                PreviewBlock::Paragraph("Hello".into())
            ])
        );
    }
}
