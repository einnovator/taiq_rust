use super::super::{PluginDescriptor, PluginKind};

pub fn descriptor() -> PluginDescriptor {
    PluginDescriptor {
        id: "code-editor",
        kind: PluginKind::Editor,
        extensions: &[
            "rs", "py", "js", "ts", "tsx", "jsx", "java", "c", "h", "cpp", "hpp", "cs", "go",
            "toml", "yaml", "yml", "json", "ipynb", "html", "css", "sh", "ps1", "sql", "xml",
        ],
    }
}
