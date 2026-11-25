#[cfg(feature = "desktop-admin")]
pub mod admin;
pub mod agents;
pub mod cli;
pub mod config;
pub mod connectors;
pub mod credentials;
pub mod filestore;
pub mod schema;
#[cfg(feature = "terminal-ui")]
pub mod terminal;
#[cfg(feature = "desktop-admin")]
pub mod ui;

#[cfg(feature = "desktop-admin")]
#[path = "../taiq_ext/plugins/mod.rs"]
pub mod plugins;
