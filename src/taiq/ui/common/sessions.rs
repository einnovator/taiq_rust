use super::{ChatAttachment, ChatMessage, QueuedChat};
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatSession {
    pub id: String,
    pub agent_id: String,
    pub title: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default = "default_model")]
    pub model_id: String,
    #[serde(default)]
    pub reasoning_effort: Option<String>,
    #[serde(default = "default_memory")]
    pub memory_enabled: bool,
    #[serde(default)]
    pub memory_start: usize,
    #[serde(default)]
    pub queue: Vec<QueuedChat>,
    #[serde(default)]
    pub draft: String,
    #[serde(default)]
    pub attachments: Vec<ChatAttachment>,
    #[serde(default)]
    pub attach_open_file: bool,
}

fn default_model() -> String {
    "auto".into()
}
fn default_memory() -> bool {
    true
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct SessionFile {
    active_id: String,
    sessions: Vec<ChatSession>,
}

pub struct SessionStore {
    path: PathBuf,
    file: SessionFile,
}

impl SessionStore {
    pub fn open(config_path: &Path) -> Result<Self> {
        let base = std::env::var_os("LOCALAPPDATA")
            .or_else(|| std::env::var_os("XDG_DATA_HOME"))
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_else(std::env::temp_dir)
            .join("taiq");
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        config_path.hash(&mut hasher);
        let path = base.join(format!("{:016x}.sessions.json", hasher.finish()));
        let file = if path.exists() {
            let data = fs::read(&path).with_context(|| format!("read {}", path.display()))?;
            match serde_json::from_slice(&data) {
                Ok(file) => file,
                Err(error) => {
                    let backup = path.with_extension(format!(
                        "corrupt-{}.json",
                        SystemTime::now()
                            .duration_since(UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                    ));
                    fs::rename(&path, &backup)
                        .with_context(|| format!("back up damaged sessions after {error}"))?;
                    eprintln!("Damaged session history moved to {}", backup.display());
                    SessionFile::default()
                }
            }
        } else {
            SessionFile::default()
        };
        Ok(Self { path, file })
    }

    pub fn sessions(&self) -> &[ChatSession] {
        &self.file.sessions
    }
    pub fn active(&self) -> Option<&ChatSession> {
        self.file
            .sessions
            .iter()
            .find(|s| s.id == self.file.active_id)
    }
    pub fn select(&mut self, id: &str) -> bool {
        if self.file.sessions.iter().any(|s| s.id == id) {
            self.file.active_id = id.to_owned();
            true
        } else {
            false
        }
    }
    pub fn new_session(&mut self, agent_id: &str) -> &ChatSession {
        let id = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
            .to_string();
        self.file.active_id = id.clone();
        self.file.sessions.push(ChatSession {
            id,
            agent_id: agent_id.to_owned(),
            title: "New chat".into(),
            messages: Vec::new(),
            model_id: default_model(),
            reasoning_effort: None,
            memory_enabled: true,
            memory_start: 0,
            queue: Vec::new(),
            draft: String::new(),
            attachments: Vec::new(),
            attach_open_file: false,
        });
        self.active().expect("new session exists")
    }
    pub fn update(
        &mut self,
        agent_id: &str,
        model_id: &str,
        reasoning_effort: Option<&str>,
        memory_enabled: bool,
        memory_start: usize,
        queue: &VecDeque<QueuedChat>,
        draft: &str,
        attachments: &[ChatAttachment],
        attach_open_file: bool,
        messages: &[ChatMessage],
    ) {
        let active = self.file.active_id.clone();
        if let Some(session) = self.file.sessions.iter_mut().find(|s| s.id == active) {
            session.agent_id = agent_id.to_owned();
            session.model_id = model_id.to_owned();
            session.reasoning_effort = reasoning_effort.map(str::to_owned);
            session.memory_enabled = memory_enabled;
            session.memory_start = memory_start;
            session.queue = queue.iter().cloned().collect();
            session.draft = draft.to_owned();
            session.attachments = attachments.to_vec();
            session.attach_open_file = attach_open_file;
            session.messages = messages.to_vec();
            if session.title == "New chat" {
                if let Some(first) = messages.iter().find(|m| m.role == super::ChatRole::User) {
                    session.title = first.text.chars().take(48).collect();
                }
            }
        }
    }
    pub fn save(&self) -> Result<()> {
        fs::create_dir_all(self.path.parent().expect("session path has parent"))?;
        fs::write(&self.path, serde_json::to_vec_pretty(&self.file)?)
            .with_context(|| format!("save {}", self.path.display()))
    }

    pub fn append_assistant_text(&mut self, id: &str, text: &str) {
        if let Some(message) = self
            .file
            .sessions
            .iter_mut()
            .find(|session| session.id == id)
            .and_then(|session| {
                session
                    .messages
                    .iter_mut()
                    .rev()
                    .find(|message| message.role == super::ChatRole::Assistant)
            })
        {
            message.text.push_str(text);
        }
    }

    pub fn message_count(&self, id: &str) -> usize {
        self.file
            .sessions
            .iter()
            .find(|session| session.id == id)
            .map(|session| session.messages.len())
            .unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persists_session_messages_and_selection() {
        let path =
            std::env::temp_dir().join(format!("taiq-sessions-test-{}.json", std::process::id()));
        let mut store = SessionStore {
            path: path.clone(),
            file: SessionFile::default(),
        };
        let id = store.new_session("iq").id.clone();
        let queue = VecDeque::from([QueuedChat {
            prompt: "next question".into(),
            attachments: Vec::new(),
            attach_open_file: false,
        }]);
        store.update(
            "iq",
            "auto",
            None,
            true,
            0,
            &queue,
            "draft for later",
            &[],
            false,
            &[ChatMessage {
                role: super::super::ChatRole::User,
                text: "Hello".into(),
            }],
        );
        store.save().unwrap();
        let saved: SessionFile = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.active_id, id);
        assert_eq!(saved.sessions[0].messages[0].text, "Hello");
        assert_eq!(saved.sessions[0].queue[0].prompt, "next question");
        assert_eq!(saved.sessions[0].draft, "draft for later");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn background_response_keeps_its_conversation_identity() {
        let mut store = SessionStore {
            path: PathBuf::new(),
            file: SessionFile::default(),
        };
        let first = store.new_session("agent-a").id.clone();
        store.update(
            "agent-a",
            "auto",
            None,
            true,
            0,
            &VecDeque::new(),
            "",
            &[],
            false,
            &[ChatMessage {
                role: super::super::ChatRole::Assistant,
                text: String::new(),
            }],
        );
        let second = store.new_session("agent-b").id.clone();
        store.append_assistant_text(&first, "response for A");
        assert_eq!(store.active().unwrap().id, second);
        assert_eq!(
            store
                .sessions()
                .iter()
                .find(|session| session.id == first)
                .unwrap()
                .messages[0]
                .text,
            "response for A"
        );
    }
}
