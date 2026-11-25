//! Lightweight web document navigation for the native workbench.
use anyhow::{Context, Result};
use regex::Regex;

#[derive(Debug, Clone)]
pub struct WebPage {
    pub url: String,
    pub title: String,
    pub text: String,
    pub links: Vec<(String, String)>,
}

pub async fn fetch(input: &str) -> Result<WebPage> {
    let input = input.trim();
    let url = if input.contains("://") {
        input.to_owned()
    } else {
        format!("https://{input}")
    };
    let parsed = reqwest::Url::parse(&url).context("invalid web address")?;
    anyhow::ensure!(
        matches!(parsed.scheme(), "http" | "https"),
        "only HTTP and HTTPS addresses are supported"
    );
    let mut response = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()?
        .get(parsed)
        .send()
        .await?
        .error_for_status()?;
    let final_url = response.url().to_string();
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await? {
        let remaining = 1_000_000usize.saturating_sub(bytes.len());
        if remaining == 0 {
            break;
        }
        bytes.extend_from_slice(&chunk[..chunk.len().min(remaining)]);
    }
    let html = String::from_utf8_lossy(&bytes).into_owned();
    let title_re = Regex::new(r"(?is)<title[^>]*>(.*?)</title>")?;
    let title = title_re
        .captures(&html)
        .map(|c| clean(&c[1]))
        .unwrap_or_else(|| final_url.clone());
    let link_re = Regex::new(r#"(?is)<a\s+[^>]*href\s*=\s*["']([^"']+)["'][^>]*>(.*?)</a>"#)?;
    let base = reqwest::Url::parse(&final_url)?;
    let links = link_re
        .captures_iter(&html)
        .take(100)
        .filter_map(|c| {
            let target = base.join(&c[1]).ok()?;
            if !matches!(target.scheme(), "http" | "https") {
                return None;
            }
            let label = clean(&c[2]);
            if label.is_empty() {
                None
            } else {
                Some((label, target.to_string()))
            }
        })
        .collect();
    let mut content = html;
    for tag in ["script", "style", "svg", "noscript"] {
        let re = Regex::new(&format!(r"(?is)<{tag}\b[^>]*>.*?</{tag}\s*>"))?;
        content = re.replace_all(&content, " ").into_owned();
    }
    let block_re = Regex::new(r"(?i)</?(p|div|section|article|h[1-6]|li|br|tr)\b[^>]*>")?;
    content = block_re.replace_all(&content, "\n").into_owned();
    let text = clean(&content);
    Ok(WebPage {
        url: final_url,
        title,
        text,
        links,
    })
}

fn clean(source: &str) -> String {
    let tags = Regex::new(r"(?s)<[^>]*>").unwrap();
    let plain = tags.replace_all(source, " ");
    let decoded = plain
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&nbsp;", " ");
    decoded
        .lines()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[tokio::test]
    async fn follows_local_document_links() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 1024];
            let _ = socket.read(&mut request).await.unwrap();
            let body =
                "<title>Example</title><main><p>Hello world</p><a href='/next'>Continue</a></main>";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            socket.write_all(response.as_bytes()).await.unwrap();
        });
        let page = fetch(&format!("http://{address}/")).await.unwrap();
        server.await.unwrap();
        assert_eq!(page.title, "Example");
        assert!(page.text.contains("Hello world"));
        assert_eq!(page.links[0].1, format!("http://{address}/next"));
    }
}
