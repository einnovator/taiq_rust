use crate::credentials::CredentialStore;
use crate::filestore::{FileStore, LocalFileStore, RemoteFileStore};
use crate::schema::ConnectorDefinition;
use anyhow::{Context, Result, bail};
use reqwest::{Client, Url};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Read-only desktop connector browser. Paths are provider-specific and never
/// interpreted as local filesystem paths.
pub struct RemoteConnector {
    definition: ConnectorDefinition,
    token: Option<String>,
    client: Client,
    project_root: PathBuf,
    config_path: PathBuf,
}

impl RemoteConnector {
    pub fn new(definition: ConnectorDefinition, config_path: &Path) -> Result<Self> {
        if definition.enable.as_ref().and_then(Value::as_bool) == Some(false)
            || definition.enabled.as_ref().and_then(Value::as_bool) == Some(false)
        {
            bail!("connector is disabled");
        }
        let id = definition
            .id
            .as_deref()
            .context("connector ID is missing")?;
        let provider = definition.provider.as_deref().unwrap_or(id);
        let inline = ["api_key", "token", "access_token", "bearer"]
            .into_iter()
            .find_map(|key| definition.credentials.get(key).and_then(Value::as_str))
            .filter(|value| !value.is_empty())
            .map(str::to_owned);
        let vault = CredentialStore::open(config_path)?;
        let token = inline
            .or_else(|| vault.token_for(id))
            .or_else(|| vault.token_for(provider));
        let client = Client::builder().timeout(Duration::from_secs(25)).build()?;
        let project_root = config_path
            .parent()
            .and_then(Path::parent)
            .unwrap_or_else(|| config_path.parent().unwrap_or(Path::new(".")))
            .to_path_buf();
        Ok(Self {
            definition,
            token,
            client,
            project_root,
            config_path: config_path.to_path_buf(),
        })
    }

    pub async fn browse(&self, path: &str) -> Result<Value> {
        if let Some(path) = path.strip_prefix("read ") {
            return Ok(json!({"path": path, "content": self.read_text(path.trim()).await?}));
        }
        if self.definition.proto.as_deref() == Some("mcp") {
            return self.mcp_browse(path).await;
        }
        match self
            .definition
            .provider
            .as_deref()
            .or(self.definition.id.as_deref())
        {
            Some("filesystem") => {
                let files = self.filesystem_store()?.list(path)?;
                Ok(json!(
                    files
                        .into_iter()
                        .map(|item| json!({"name":item.name,"path":item.path,"is_dir":item.is_dir}))
                        .collect::<Vec<_>>()
                ))
            }
            Some(
                "aws_s3" | "s3_compatible" | "minio" | "wasabi" | "cloudflare_r2" | "azure_blob"
                | "gcp_storage",
            ) => {
                let files = self.storage_store()?.list(path)?;
                Ok(json!(
                    files
                        .into_iter()
                        .map(|item| json!({"name":item.name,"path":item.path,"is_dir":item.is_dir}))
                        .collect::<Vec<_>>()
                ))
            }
            Some("gitlab") => self.gitlab_browse(path).await,
            Some("github") => self.github_browse(path).await,
            Some("brave_search") => self.brave_search(path).await,
            Some("tavily") => self.tavily_search(path).await,
            Some(provider) => bail!("NotImplemented: connector provider {provider}"),
            None => bail!("connector provider is missing"),
        }
    }

    pub async fn read_text(&self, path: &str) -> Result<String> {
        if self.definition.provider.as_deref() == Some("filesystem") {
            return self.filesystem_store()?.read_text(path);
        }
        if matches!(
            self.definition
                .provider
                .as_deref()
                .or(self.definition.id.as_deref()),
            Some(
                "aws_s3"
                    | "s3_compatible"
                    | "minio"
                    | "wasabi"
                    | "cloudflare_r2"
                    | "azure_blob"
                    | "gcp_storage"
            )
        ) {
            return self.storage_store()?.read_text(path);
        }
        if self.definition.proto.as_deref() == Some("mcp") {
            return Ok(serde_json::to_string_pretty(
                &self.mcp_browse(&format!("read:{path}")).await?,
            )?);
        }
        let provider = self
            .definition
            .provider
            .as_deref()
            .or(self.definition.id.as_deref());
        let mut request = match provider {
            Some("gitlab") => {
                let (project, file) = path
                    .split_once(':')
                    .context("GitLab file path must be project:file")?;
                if file.trim_matches('/').is_empty() {
                    bail!("GitLab file path is empty");
                }
                let mut url = self.endpoint(
                    "https://gitlab.com",
                    &[
                        "api",
                        "v4",
                        "projects",
                        project,
                        "repository",
                        "files",
                        file.trim_matches('/'),
                        "raw",
                    ],
                )?;
                url.query_pairs_mut().append_pair("ref", "HEAD");
                let mut request = self.client.get(url);
                if let Some(token) = &self.token {
                    request = request.header("PRIVATE-TOKEN", token);
                }
                request
            }
            Some("github") => {
                let parts: Vec<_> = path.splitn(3, '/').collect();
                if parts.len() != 3 {
                    bail!("GitHub file path must be owner/repo/file");
                }
                let mut segments = vec!["repos", parts[0], parts[1], "contents"];
                segments.extend(parts[2].split('/').filter(|s| !s.is_empty()));
                let url = self.endpoint("https://api.github.com", &segments)?;
                let mut request = self
                    .client
                    .get(url)
                    .header("User-Agent", "taiq-rust-desktop")
                    .header("Accept", "application/vnd.github.raw+json");
                if let Some(token) = &self.token {
                    request = request.bearer_auth(token);
                }
                request
            }
            Some(provider) => bail!("NotImplemented: read from connector provider {provider}"),
            None => bail!("connector provider is missing"),
        };
        request = request.header("Accept-Encoding", "identity");
        let response = request.send().await?.error_for_status()?;
        if response.content_length().is_some_and(|n| n > 10_000_000) {
            bail!("remote file exceeds 10 MB");
        }
        let bytes = response.bytes().await?;
        if bytes.len() > 10_000_000 {
            bail!("remote file exceeds 10 MB");
        }
        Ok(String::from_utf8(bytes.to_vec())?)
    }

    fn filesystem_store(&self) -> Result<LocalFileStore> {
        let root = self
            .definition
            .options
            .get("root")
            .and_then(Value::as_str)
            .context("filesystem connector root is missing")?;
        let root = Path::new(root);
        let resolved = if root.is_absolute() {
            root.to_path_buf()
        } else {
            self.project_root.join(root)
        };
        LocalFileStore::new_with_mode(resolved, true)
    }

    fn storage_store(&self) -> Result<RemoteFileStore> {
        let provider = self
            .definition
            .provider
            .as_deref()
            .or(self.definition.id.as_deref())
            .context("storage connector provider is missing")?;
        let kind = match provider {
            "azure_blob" => "azure",
            "gcp_storage" => "gcs",
            _ => "s3",
        };
        let mut options = serde_json::to_value(&self.definition.options)?;
        if let Some(object) = options.as_object_mut() {
            for (key, value) in &self.definition.credentials {
                object.insert(key.clone(), value.clone());
            }
        }
        let definition =
            json!({"id": self.definition.id, "type": kind, "options": options, "read-only": true});
        RemoteFileStore::from_config(&definition, &self.config_path)
    }

    fn base(&self, fallback: &str) -> Result<Url> {
        let url = self
            .definition
            .options
            .get("url")
            .and_then(Value::as_str)
            .unwrap_or(fallback);
        let base = Url::parse(url).context("invalid connector URL")?;
        if base.scheme() != "https"
            && !(base.scheme() == "http"
                && self
                    .definition
                    .options
                    .get("allow_insecure_http")
                    .and_then(Value::as_bool)
                    == Some(true))
        {
            bail!("connector URL must use HTTPS (or explicitly allow HTTP)");
        }
        Ok(base)
    }

    fn endpoint(&self, fallback: &str, segments: &[&str]) -> Result<Url> {
        let mut url = self.base(fallback)?;
        url.path_segments_mut()
            .map_err(|_| anyhow::anyhow!("invalid connector URL path"))?
            .pop_if_empty()
            .extend(segments.iter().copied());
        Ok(url)
    }

    async fn gitlab_browse(&self, path: &str) -> Result<Value> {
        // Root lists projects. A project path (numeric ID or namespace/name)
        // lists its repository; a second path component selects a directory.
        let mut parts = path.splitn(2, ':');
        let project = parts.next().unwrap_or("").trim();
        let folder = parts.next().unwrap_or("").trim_matches('/');
        let mut url = if project.is_empty() {
            let mut url = self.endpoint("https://gitlab.com", &["api", "v4", "projects"])?;
            url.query_pairs_mut()
                .append_pair("membership", "true")
                .append_pair("per_page", "100");
            url
        } else {
            let mut url = self.endpoint(
                "https://gitlab.com",
                &["api", "v4", "projects", project, "repository", "tree"],
            )?;
            url.query_pairs_mut().append_pair("per_page", "100");
            if !folder.is_empty() {
                url.query_pairs_mut().append_pair("path", folder);
            }
            url
        };
        // A user-configured GitLab base may include a trailing slash; reqwest
        // handles percent encoding of project IDs and nested namespaces.
        url.set_fragment(None);
        let mut request = self.client.get(url);
        if let Some(token) = &self.token {
            request = request.header("PRIVATE-TOKEN", token);
        }
        let response = request.send().await?.error_for_status()?;
        Ok(response.json().await?)
    }

    async fn github_browse(&self, path: &str) -> Result<Value> {
        let parts: Vec<_> = path.splitn(3, '/').filter(|s| !s.is_empty()).collect();
        let url = if parts.len() < 2 {
            self.endpoint("https://api.github.com", &["user", "repos"])?
        } else {
            let mut segments = vec!["repos", parts[0], parts[1], "contents"];
            if let Some(folder) = parts.get(2) {
                segments.extend(folder.split('/').filter(|s| !s.is_empty()));
            }
            self.endpoint("https://api.github.com", &segments)?
        };
        let mut request = self
            .client
            .get(url)
            .header("User-Agent", "taiq-rust-desktop")
            .header("Accept", "application/vnd.github+json");
        if let Some(token) = &self.token {
            request = request.bearer_auth(token);
        }
        Ok(request.send().await?.error_for_status()?.json().await?)
    }

    async fn brave_search(&self, query: &str) -> Result<Value> {
        if query.trim().is_empty() {
            bail!("search query is required");
        }
        let key = self
            .token
            .as_deref()
            .context("Brave Search API key is missing")?;
        let mut url = self.endpoint(
            "https://api.search.brave.com",
            &["res", "v1", "web", "search"],
        )?;
        url.query_pairs_mut().append_pair("q", query);
        Ok(self
            .client
            .get(url)
            .header("X-Subscription-Token", key)
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn tavily_search(&self, query: &str) -> Result<Value> {
        if query.trim().is_empty() {
            bail!("search query is required");
        }
        let key = self.token.as_deref().context("Tavily API key is missing")?;
        let url = self.endpoint("https://api.tavily.com", &["search"])?;
        Ok(self
            .client
            .post(url)
            .bearer_auth(key)
            .json(&json!({"query": query}))
            .send()
            .await?
            .error_for_status()?
            .json()
            .await?)
    }

    async fn mcp_browse(&self, path: &str) -> Result<Value> {
        let raw = self
            .definition
            .other
            .get("url")
            .and_then(Value::as_str)
            .context("MCP connector URL is missing")?;
        let url = Url::parse(raw)?;
        if !matches!(url.scheme(), "http" | "https") {
            bail!("MCP URL must be HTTP(S)");
        }
        let auth = self
            .definition
            .other
            .get("auth")
            .and_then(|v| v.get("bearer"))
            .and_then(Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
            .or_else(|| self.token.clone());
        let initialize = self
            .mcp_request(
                &url,
                auth.as_deref(),
                None,
                json!({
                    "jsonrpc":"2.0", "id":1, "method":"initialize", "params": {
                        "protocolVersion":"2025-03-26", "capabilities":{},
                        "clientInfo":{"name":"taiq","version":"0.1.0"}
                    }
                }),
            )
            .await?;
        let session = initialize.1;
        let mut notification = self
            .client
            .post(url.clone())
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .json(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        if let Some(auth) = auth.as_deref() {
            notification = notification.bearer_auth(auth);
        }
        if let Some(session) = session.as_deref() {
            notification = notification.header("Mcp-Session-Id", session);
        }
        notification.send().await?.error_for_status()?;
        let (method, params) = if let Some(uri) = path.strip_prefix("read:") {
            ("resources/read", json!({"uri":uri}))
        } else if path == "tools" {
            ("tools/list", json!({}))
        } else {
            ("resources/list", json!({}))
        };
        let result = self
            .mcp_request(
                &url,
                auth.as_deref(),
                session.as_deref(),
                json!({"jsonrpc":"2.0", "id":2, "method":method, "params":params}),
            )
            .await?
            .0;
        if let Some(error) = result.get("error") {
            bail!("MCP error: {error}");
        }
        Ok(result.get("result").cloned().unwrap_or(result))
    }

    async fn mcp_request(
        &self,
        url: &Url,
        auth: Option<&str>,
        session: Option<&str>,
        body: Value,
    ) -> Result<(Value, Option<String>)> {
        let mut request = self
            .client
            .post(url.clone())
            .header("Accept", "application/json, text/event-stream")
            .header("Content-Type", "application/json")
            .json(&body);
        if let Some(auth) = auth {
            request = request.bearer_auth(auth);
        }
        if let Some(session) = session {
            request = request.header("Mcp-Session-Id", session);
        }
        let response = request.send().await?.error_for_status()?;
        let session = response
            .headers()
            .get("Mcp-Session-Id")
            .and_then(|v| v.to_str().ok())
            .map(str::to_owned);
        let is_sse = response
            .headers()
            .get("Content-Type")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/event-stream"));
        let text = response.text().await?;
        let payload = if is_sse {
            text.lines()
                .find_map(|line| line.strip_prefix("data:").map(str::trim_start))
                .context("MCP stream contained no data event")?
        } else {
            &text
        };
        let result: Value = serde_json::from_str(payload)?;
        if let Some(error) = result.get("error") {
            bail!("MCP error: {error}");
        }
        Ok((result, session))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn browses_configured_gitlab_endpoint() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 4096];
            let count = socket.read(&mut request).await.unwrap();
            let text = String::from_utf8_lossy(&request[..count]);
            assert!(text.contains("/api/v4/projects?membership=true&per_page=100"));
            socket.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 12\r\nConnection: close\r\n\r\n[{\"id\":123}]").await.unwrap();
        });
        let mut options = BTreeMap::new();
        options.insert("url".into(), Value::String(format!("http://{address}")));
        options.insert("allow_insecure_http".into(), Value::Bool(true));
        let connector = ConnectorDefinition {
            id: Some("test-gitlab".into()),
            provider: Some("gitlab".into()),
            options,
            ..Default::default()
        };
        let client = RemoteConnector::new(connector, Path::new("test-config.yaml")).unwrap();
        assert_eq!(client.browse("").await.unwrap()[0]["id"], 123);
        server.await.unwrap();
    }
}
