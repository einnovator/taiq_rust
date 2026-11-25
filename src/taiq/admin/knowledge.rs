use anyhow::{Result, bail};
use serde::{Deserialize, Serialize};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Knowledge {
    pub id: String,
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub markdown: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct KnowledgeHit {
    pub id: String,
    pub name: Option<String>,
    pub score: usize,
    pub excerpt: String,
}

pub struct KnowledgeStore {
    path: PathBuf,
}

impl KnowledgeStore {
    pub fn open(config_path: &Path) -> Self {
        let root = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .or_else(|| {
                std::env::var_os("HOME")
                    .map(|v| PathBuf::from(v).join(".local/share").into_os_string())
            })
            .map(PathBuf::from)
            .unwrap_or_else(std::env::temp_dir)
            .join("taiq");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        config_path.hash(&mut hasher);
        Self {
            path: root.join(format!("{:016x}.knowledge.json", hasher.finish())),
        }
    }

    pub fn list(&self) -> Result<Vec<Knowledge>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        Ok(serde_json::from_slice(&fs::read(&self.path)?)?)
    }

    pub fn replace(&self, items: &[Knowledge]) -> Result<()> {
        let mut ids = std::collections::HashSet::new();
        for item in items {
            if item.id.trim().is_empty() || !ids.insert(&item.id) {
                bail!("knowledge IDs must be unique and nonempty");
            }
        }
        fs::create_dir_all(self.path.parent().expect("knowledge path has parent"))?;
        let temp = self.path.with_extension("json.tmp");
        fs::write(&temp, serde_json::to_vec_pretty(items)?)?;
        fs::rename(temp, &self.path)?;
        Ok(())
    }

    pub fn search(&self, query: &str) -> Result<Vec<KnowledgeHit>> {
        let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let mut hits = Vec::new();
        for item in self.list()? {
            let text = format!(
                "{} {} {} {}",
                item.name.as_deref().unwrap_or(""),
                item.description.as_deref().unwrap_or(""),
                item.tags.join(" "),
                item.markdown
            )
            .to_lowercase();
            let score = terms
                .iter()
                .map(|term| text.matches(term).count())
                .sum::<usize>();
            if score > 0 {
                let excerpt = item.markdown.chars().take(400).collect();
                hits.push(KnowledgeHit {
                    id: item.id,
                    name: item.name,
                    score,
                    excerpt,
                });
            }
        }
        hits.sort_by(|a, b| b.score.cmp(&a.score).then_with(|| a.id.cmp(&b.id)));
        hits.truncate(50);
        Ok(hits)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn searches_saved_knowledge() {
        let path = std::env::temp_dir()
            .join(format!("taiq-knowledge-test-{}", std::process::id()))
            .join("config.yaml");
        let store = KnowledgeStore::open(&path);
        store
            .replace(&[Knowledge {
                id: "one".into(),
                name: Some("Rust".into()),
                description: None,
                tags: vec!["code".into()],
                markdown: "Rust desktop app".into(),
            }])
            .unwrap();
        assert_eq!(store.search("desktop").unwrap()[0].id, "one");
        let _ = fs::remove_file(store.path);
    }
}
