use crate::connectors::remote::RemoteConnector;
use crate::credentials::CredentialStore;
use crate::schema::{ConnectorDefinition, ForgeConfig, ModelProviderConfig};
use anyhow::{Result, bail};
use futures::{StreamExt, stream};
use reqwest::Client;
use serde::Serialize;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug, Serialize)]
struct Probe {
    id: String,
    status: &'static str,
    detail: String,
}

impl Probe {
    fn ok(id: String) -> Self {
        Self {
            id,
            status: "ok",
            detail: "connected".into(),
        }
    }
    fn failed(id: String, detail: impl Into<String>) -> Self {
        Self {
            id,
            status: "failed",
            detail: detail.into(),
        }
    }
    fn unsupported(id: String) -> Self {
        Self {
            id,
            status: "unsupported",
            detail: "no Rust probe for this provider".into(),
        }
    }
}

pub async fn run(config: &ForgeConfig, path: &Path, with_connectors: bool) -> Result<()> {
    let models = config
        .models
        .as_ref()
        .map(|m| {
            m.providers
                .iter()
                .filter(|p| p.other.get("enabled").and_then(Value::as_bool) != Some(false))
                .cloned()
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let config_path = path.to_path_buf();
    let config = config.clone();
    let mut model_results: Vec<Probe> = stream::iter(models.into_iter().map(|provider| {
        let config = config.clone();
        let path = config_path.clone();
        async move { probe_model(provider, &config, &path).await }
    }))
    .buffer_unordered(8)
    .collect()
    .await;
    model_results.sort_by(|a, b| a.id.cmp(&b.id));

    let mut connector_results = Vec::new();
    if with_connectors {
        let definitions: Vec<_> = config
            .connectors
            .iter()
            .filter(|item| {
                item.enable.as_ref().and_then(Value::as_bool) != Some(false)
                    && item.enabled.as_ref().and_then(Value::as_bool) != Some(false)
            })
            .cloned()
            .collect();
        connector_results = stream::iter(definitions.into_iter().map(|definition| {
            let path = config_path.clone();
            async move { probe_connector(definition, path).await }
        }))
        .buffer_unordered(8)
        .collect()
        .await;
        connector_results.sort_by(|a, b| a.id.cmp(&b.id));
    }
    let failures = model_results
        .iter()
        .chain(&connector_results)
        .filter(|item| item.status == "failed")
        .count();
    super::write_line(
        &json!({"ok":failures == 0,"failed":failures,"models":model_results,"connectors":connector_results}),
    )?;
    if failures > 0 {
        bail!("{failures} connectivity probes failed");
    }
    Ok(())
}

async fn probe_model(provider: ModelProviderConfig, config: &ForgeConfig, path: &Path) -> Probe {
    let id = provider.name.clone();
    let (base, route, auth) = match id.as_str() {
        "openai" => ("https://api.openai.com/v1", "models", "bearer"),
        "openrouter" => ("https://openrouter.ai/api/v1", "models", "bearer"),
        "anthropic" => ("https://api.anthropic.com/v1", "models", "anthropic"),
        "google" | "gemini" => (
            "https://generativelanguage.googleapis.com/v1beta",
            "models",
            "google",
        ),
        "ollama" => ("http://localhost:11434", "api/tags", "none"),
        _ => return Probe::unsupported(id),
    };
    let token = if auth == "none" {
        None
    } else {
        provider_token(&provider, config, path)
    };
    if auth != "none" && token.is_none() {
        return Probe::failed(id, "credential not configured");
    }
    let url = format!(
        "{}/{}",
        provider
            .url
            .as_deref()
            .filter(|s| !s.is_empty())
            .unwrap_or(base)
            .trim_end_matches('/'),
        route
    );
    let client = match Client::builder().timeout(Duration::from_secs(8)).build() {
        Ok(client) => client,
        Err(error) => return Probe::failed(id, error.to_string()),
    };
    let mut request = client.get(url);
    match auth {
        "bearer" => request = request.bearer_auth(token.unwrap()),
        "anthropic" => {
            request = request
                .header("x-api-key", token.unwrap())
                .header("anthropic-version", "2023-06-01")
        }
        "google" => request = request.header("x-goog-api-key", token.unwrap()),
        _ => {}
    }
    match request.send().await {
        Ok(response) if response.status().is_success() => Probe::ok(id),
        Ok(response) => Probe::failed(id, format!("HTTP {}", response.status())),
        Err(error) => Probe::failed(id, error.to_string()),
    }
}

fn provider_token(
    provider: &ModelProviderConfig,
    config: &ForgeConfig,
    path: &Path,
) -> Option<String> {
    let env_names: &[&str] = match provider.name.as_str() {
        "openai" => &["OPENAI_API_KEY"],
        "openrouter" => &["OPENROUTER_API_KEY"],
        "anthropic" => &["ANTHROPIC_API_KEY"],
        "google" | "gemini" => &["GOOGLE_API_KEY", "GEMINI_API_KEY"],
        _ => &[],
    };
    env_names
        .iter()
        .find_map(|name| std::env::var(name).ok().filter(|value| !value.is_empty()))
        .or_else(|| CredentialStore::open(path).ok()?.token_for(&provider.name))
        .or_else(|| {
            config
                .models
                .as_ref()?
                .providers
                .iter()
                .find(|item| item.name == provider.name)?
                .token
                .clone()
                .filter(|value| !value.is_empty())
        })
}

async fn probe_connector(definition: ConnectorDefinition, config_path: PathBuf) -> Probe {
    let id = definition.id.clone().unwrap_or_else(|| "<unnamed>".into());
    let provider = definition.provider.as_deref().or(definition.id.as_deref());
    if definition.proto.as_deref() != Some("mcp")
        && !matches!(provider, Some("github" | "gitlab" | "filesystem"))
    {
        return Probe::unsupported(id);
    }
    let connector = match RemoteConnector::new(definition, &config_path) {
        Ok(connector) => connector,
        Err(error) => return Probe::failed(id, error.to_string()),
    };
    match tokio::time::timeout(Duration::from_secs(8), connector.browse("")).await {
        Ok(Ok(_)) => Probe::ok(id),
        Ok(Err(error)) => Probe::failed(id, error.to_string()),
        Err(_) => Probe::failed(id, "timed out after 8 seconds"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn probes_configured_local_model_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut buffer = [0u8; 2048];
            let count = socket.read(&mut buffer).await.unwrap();
            assert!(String::from_utf8_lossy(&buffer[..count]).contains("GET /api/tags"));
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
        });
        let provider = ModelProviderConfig {
            name: "ollama".into(),
            id: None,
            api: None,
            url: Some(format!("http://{address}")),
            token: None,
            litellm_prefix: None,
            other: BTreeMap::new(),
        };
        let result = probe_model(provider, &ForgeConfig::default(), Path::new("test.yaml")).await;
        assert_eq!(result.status, "ok");
        server.await.unwrap();
    }
}
