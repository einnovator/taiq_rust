use super::PreviewBlock;
use serde_json::Value;

pub fn preview(path: &str, source: &str) -> Option<Vec<PreviewBlock>> {
    let extension = path.rsplit('.').next()?.to_ascii_lowercase();
    match extension.as_str() {
        "csv" => Some(csv_preview(source)),
        "ipynb" => notebook_preview(source),
        "json" => {
            if let Some(blocks) = super::tiptap_preview(source) {
                return Some(blocks);
            }
            let value: Value = serde_json::from_str(source).ok()?;
            Some(vec![PreviewBlock::Code(
                serde_json::to_string_pretty(&value).ok()?,
            )])
        }
        _ => None,
    }
}

fn csv_preview(source: &str) -> Vec<PreviewBlock> {
    let mut rows: Vec<Vec<String>> = vec![Vec::new()];
    let mut cell = String::new();
    let mut quoted = false;
    let mut chars = source.chars().peekable();
    while let Some(ch) = chars.next() {
        match ch {
            '"' if quoted && chars.peek() == Some(&'"') => {
                cell.push('"');
                chars.next();
            }
            '"' => quoted = !quoted,
            ',' if !quoted => {
                rows.last_mut().unwrap().push(std::mem::take(&mut cell));
            }
            '\n' if !quoted => {
                rows.last_mut().unwrap().push(std::mem::take(&mut cell));
                if rows.len() >= 101 {
                    break;
                }
                rows.push(Vec::new());
            }
            '\r' if !quoted => {}
            _ => cell.push(ch),
        }
    }
    if !cell.is_empty() || !rows.last().unwrap().is_empty() {
        rows.last_mut().unwrap().push(cell);
    }
    let rows: Vec<_> = rows.into_iter().filter(|row| !row.is_empty()).collect();
    let mut blocks = vec![PreviewBlock::Heading(
        2,
        format!("CSV · {} preview rows", rows.len().saturating_sub(1)),
    )];
    let table = rows
        .into_iter()
        .map(|row| row.join("  |  "))
        .collect::<Vec<_>>()
        .join("\n");
    blocks.push(PreviewBlock::Code(table));
    blocks
}

fn notebook_preview(source: &str) -> Option<Vec<PreviewBlock>> {
    let root: Value = serde_json::from_str(source).ok()?;
    let cells = root.get("cells")?.as_array()?;
    let mut blocks = vec![PreviewBlock::Heading(
        1,
        format!("Notebook · {} cells", cells.len()),
    )];
    for (index, cell) in cells.iter().take(100).enumerate() {
        let kind = cell
            .get("cell_type")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let text = cell.get("source").map(join_text).unwrap_or_default();
        blocks.push(PreviewBlock::Heading(
            3,
            format!("Cell {} · {kind}", index + 1),
        ));
        if kind == "markdown" {
            blocks.extend(super::markdown_preview(&text));
        } else {
            blocks.push(PreviewBlock::Code(text));
        }
        if let Some(outputs) = cell.get("outputs").and_then(Value::as_array) {
            for output in outputs.iter().take(8) {
                let text = output
                    .get("text")
                    .or_else(|| output.get("data").and_then(|d| d.get("text/plain")));
                if let Some(text) = text {
                    blocks.push(PreviewBlock::Code(join_text(text)));
                }
            }
        }
    }
    Some(blocks)
}

fn join_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Array(parts) => parts.iter().filter_map(Value::as_str).collect::<String>(),
        _ => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn previews_csv_quoted_fields_and_notebook() {
        let csv = preview("sample.csv", "name,note\na,\"one,two\"\n").unwrap();
        assert!(matches!(&csv[1], PreviewBlock::Code(text) if text.contains("one,two")));
        let notebook = preview(
            "x.ipynb",
            r#"{"cells":[{"cell_type":"code","source":["print(1)"],"outputs":[]}]}"#,
        )
        .unwrap();
        assert!(
            notebook
                .iter()
                .any(|b| matches!(b, PreviewBlock::Code(text) if text == "print(1)"))
        );
    }
}
