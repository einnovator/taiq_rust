use super::{
    super::{PluginDescriptor, PluginKind},
    PreviewBlock,
};

pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "markdown-viewer",
        kind: PluginKind::Editor,
        extensions: &["md", "mdx"],
    }
}

pub fn parse(source: &str) -> Vec<PreviewBlock> {
    let mut blocks = Vec::new();
    let mut paragraph = String::new();
    let mut code = String::new();
    let mut in_code = false;
    let flush = |blocks: &mut Vec<PreviewBlock>, paragraph: &mut String| {
        if !paragraph.is_empty() {
            blocks.push(PreviewBlock::Paragraph(std::mem::take(paragraph)));
        }
    };
    for line in source.lines() {
        if line.starts_with("```") {
            flush(&mut blocks, &mut paragraph);
            if in_code {
                blocks.push(PreviewBlock::Code(std::mem::take(&mut code)));
            }
            in_code = !in_code;
        } else if in_code {
            code.push_str(line);
            code.push('\n');
        } else if line.trim_start().starts_with("> ") {
            flush(&mut blocks, &mut paragraph);
            blocks.push(PreviewBlock::Quote(line.trim_start()[2..].to_owned()));
        } else if line.trim() == "---" || line.trim() == "***" {
            flush(&mut blocks, &mut paragraph);
            blocks.push(PreviewBlock::Rule);
        } else if line.trim_start().starts_with("- ") || line.trim_start().starts_with("* ") {
            flush(&mut blocks, &mut paragraph);
            blocks.push(PreviewBlock::ListItem(line.trim_start()[2..].to_owned()));
        } else if let Some((number, item)) = line.trim_start().split_once(". ") {
            if number.parse::<usize>().is_ok() {
                flush(&mut blocks, &mut paragraph);
                blocks.push(PreviewBlock::ListItem(format!("{number}. {item}")));
            } else if !line.trim().is_empty() {
                if !paragraph.is_empty() {
                    paragraph.push(' ');
                }
                paragraph.push_str(line.trim());
            }
        } else if let Some((marks, title)) = line.split_once(' ') {
            if !marks.is_empty() && marks.len() <= 6 && marks.bytes().all(|b| b == b'#') {
                flush(&mut blocks, &mut paragraph);
                blocks.push(PreviewBlock::Heading(marks.len() as u8, title.to_owned()));
            } else if !line.trim().is_empty() {
                if !paragraph.is_empty() {
                    paragraph.push(' ');
                }
                paragraph.push_str(line.trim());
            }
        } else if line.trim().is_empty() {
            flush(&mut blocks, &mut paragraph);
        } else {
            if !paragraph.is_empty() {
                paragraph.push(' ');
            }
            paragraph.push_str(line.trim());
        }
    }
    flush(&mut blocks, &mut paragraph);
    if in_code && !code.is_empty() {
        blocks.push(PreviewBlock::Code(code));
    }
    blocks
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_markdown_blocks_for_both_renderers() {
        assert_eq!(
            parse("# Title\n\nHello world\n\n```rs\nlet x = 1;\n```"),
            vec![
                PreviewBlock::Heading(1, "Title".into()),
                PreviewBlock::Paragraph("Hello world".into()),
                PreviewBlock::Code("let x = 1;\n".into()),
            ]
        );
    }
}
