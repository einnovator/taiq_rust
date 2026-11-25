pub mod connectors;
pub mod editors;
pub mod filestores;
pub mod viewers;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKind {
    Editor,
    Viewer,
    Connector,
    Filestore,
}

#[derive(Debug, Clone, Copy)]
pub struct PluginDescriptor {
    pub id: &'static str,
    pub kind: PluginKind,
    pub extensions: &'static [&'static str],
}

pub fn for_file(path: &str) -> Option<PluginDescriptor> {
    let ext = path.rsplit('.').next()?.to_ascii_lowercase();
    editors::all()
        .into_iter()
        .chain(viewers::all())
        .find(|plugin| plugin.extensions.contains(&ext.as_str()))
}

pub fn all() -> Vec<PluginDescriptor> {
    editors::all()
        .into_iter()
        .chain(viewers::all())
        .chain(connectors::all())
        .chain(filestores::all())
        .collect()
}
