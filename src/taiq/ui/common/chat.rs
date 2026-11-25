use crate::agents::{adk::build_agent, find_agent};
use crate::schema::ForgeConfig;
use adk_rust::prelude::*;
use adk_rust::session::{CreateRequest, SessionService};
use anyhow::Result;
use futures::StreamExt;
use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use tokio::sync::oneshot;

pub enum ChatUpdate {
    Text(String),
    ToolCall(String, String),
    ToolResult(String, String),
    Reasoning(String),
    Approval(u64, String, String),
    Error(String),
    Done,
}

pub struct ChatBridge {
    runtime: tokio::runtime::Runtime,
    runner: Arc<Runner>,
    session_id: String,
    sender: mpsc::Sender<ChatUpdate>,
    receiver: Mutex<mpsc::Receiver<ChatUpdate>>,
    current_task: Mutex<Option<tokio::task::JoinHandle<()>>>,
    pending_approvals:
        Arc<Mutex<HashMap<u64, oneshot::Sender<adk_rust::ToolConfirmationDecision>>>>,
}

#[derive(Debug)]
struct DesktopConfirmationHandler {
    sender: mpsc::Sender<ChatUpdate>,
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<adk_rust::ToolConfirmationDecision>>>>,
    next_id: AtomicU64,
}

#[adk_rust::async_trait]
impl adk_rust::ToolConfirmationHandler for DesktopConfirmationHandler {
    async fn decide(
        &self,
        request: &adk_rust::ToolConfirmationRequest,
    ) -> adk_rust::Result<adk_rust::ToolConfirmationDecision> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (sender, receiver) = oneshot::channel();
        self.pending
            .lock()
            .expect("approval mutex")
            .insert(id, sender);
        let _ = self.sender.send(ChatUpdate::Approval(
            id,
            request.tool_name.clone(),
            request.args.to_string(),
        ));
        Ok(receiver
            .await
            .unwrap_or(adk_rust::ToolConfirmationDecision::Deny))
    }
}

impl ChatBridge {
    pub fn new(
        config: &ForgeConfig,
        path: &Path,
        agent_id: &str,
        model_id: &str,
        reasoning_effort: Option<&str>,
    ) -> Result<Self> {
        let mut config = config.clone();
        let mut definition = find_agent(&config, agent_id)?.clone();
        if model_id != "auto" {
            let selected = config
                .models
                .as_ref()
                .and_then(|m| {
                    m.models.iter().find(|candidate| {
                        candidate.id.as_deref() == Some(model_id) || candidate.name == model_id
                    })
                })
                .ok_or_else(|| anyhow::anyhow!("unknown model: {model_id}"))?;
            definition.models = vec![crate::schema::AgentModelConfig {
                name: selected.name.clone(),
                provider: Some(selected.provider.clone()),
                other: Default::default(),
            }];
        }
        if let Some(effort) = reasoning_effort {
            if let Some(model) =
                crate::ui::common::model_picker::effective(&config, agent_id, model_id)
            {
                let name = model.name.clone();
                if let Some(configured) = config.models.as_mut().and_then(|models| {
                    models
                        .models
                        .iter_mut()
                        .find(|candidate| candidate.name == name)
                }) {
                    configured
                        .other
                        .insert("reasoning_default".into(), effort.into());
                }
            }
        }
        let agent = build_agent(&definition, &config, path)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()?;
        let service: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
        let session = runtime.block_on(service.create(CreateRequest {
            app_name: "taiq-desktop".into(),
            user_id: "desktop".into(),
            session_id: None,
            state: HashMap::new(),
        }))?;
        let (sender, receiver) = mpsc::channel();
        let pending_approvals = Arc::new(Mutex::new(HashMap::new()));
        let confirmation = DesktopConfirmationHandler {
            sender: sender.clone(),
            pending: pending_approvals.clone(),
            next_id: AtomicU64::new(1),
        };
        let runner = Arc::new(
            Runner::builder()
                .app_name("taiq-desktop")
                .agent(agent)
                .session_service(service)
                .run_config(
                    RunConfig::builder()
                        .tool_confirmation_handler(Arc::new(confirmation))
                        .build(),
                )
                .build()?,
        );
        Ok(Self {
            runtime,
            runner,
            session_id: session.id().to_string(),
            sender,
            receiver: Mutex::new(receiver),
            current_task: Mutex::new(None),
            pending_approvals,
        })
    }

    pub fn submit(&self, prompt: String) {
        let runner = self.runner.clone();
        let session_id = self.session_id.clone();
        let sender = self.sender.clone();
        let task = self.runtime.spawn(async move {
            let result = run_turn(runner, &session_id, &prompt, &sender).await;
            if let Err(error) = result {
                let _ = sender.send(ChatUpdate::Error(error.to_string()));
            }
            let _ = sender.send(ChatUpdate::Done);
        });
        if let Ok(mut current) = self.current_task.lock() {
            *current = Some(task);
        }
    }

    pub fn cancel(&self) {
        self.pending_approvals
            .lock()
            .expect("approval mutex")
            .clear();
        if let Ok(mut current) = self.current_task.lock() {
            if let Some(task) = current.take() {
                task.abort();
            }
        }
    }

    pub fn decide(&self, id: u64, approve: bool) -> bool {
        let Some(sender) = self
            .pending_approvals
            .lock()
            .expect("approval mutex")
            .remove(&id)
        else {
            return false;
        };
        sender
            .send(if approve {
                adk_rust::ToolConfirmationDecision::Approve
            } else {
                adk_rust::ToolConfirmationDecision::Deny
            })
            .is_ok()
    }

    pub fn try_recv(&self) -> Option<ChatUpdate> {
        self.receiver.lock().ok()?.try_recv().ok()
    }

    pub fn is_running(&self) -> bool {
        self.current_task
            .lock()
            .ok()
            .and_then(|task| task.as_ref().map(|task| !task.is_finished()))
            .unwrap_or(false)
    }
}

async fn run_turn(
    runner: Arc<Runner>,
    session_id: &str,
    prompt: &str,
    sender: &mpsc::Sender<ChatUpdate>,
) -> Result<()> {
    let mut stream = runner
        .run_str(
            "desktop",
            session_id,
            Content::new("user").with_text(prompt),
        )
        .await?;
    let mut deltas = adk_rust::EventTextDeltas::default();
    while let Some(event) = stream.next().await {
        let event = event?;
        let event = deltas.push(&event);
        if let Some(content) = event.content() {
            for part in &content.parts {
                match part {
                    Part::Text { text } if !text.is_empty() => {
                        let _ = sender.send(ChatUpdate::Text(text.clone()));
                    }
                    Part::FunctionCall { name, args, .. } => {
                        let _ = sender.send(ChatUpdate::ToolCall(name.clone(), args.to_string()));
                    }
                    Part::FunctionResponse {
                        function_response, ..
                    } => {
                        let _ = sender.send(ChatUpdate::ToolResult(
                            function_response.name.clone(),
                            function_response.response.to_string(),
                        ));
                    }
                    Part::Thinking { thinking, .. } if !thinking.is_empty() => {
                        let _ = sender.send(ChatUpdate::Reasoning(thinking.clone()));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use adk_rust::ToolConfirmationHandler;

    #[tokio::test]
    async fn confirmation_waits_for_desktop_decision() {
        let (sender, receiver) = mpsc::channel();
        let pending = Arc::new(Mutex::new(HashMap::new()));
        let handler = Arc::new(DesktopConfirmationHandler {
            sender,
            pending: pending.clone(),
            next_id: AtomicU64::new(1),
        });
        let request = adk_rust::ToolConfirmationRequest {
            tool_name: "write_file".into(),
            function_call_id: Some("call-1".into()),
            args: serde_json::json!({"path": "a.txt"}),
        };
        let task = tokio::spawn(async move { handler.decide(&request).await.unwrap() });
        tokio::task::yield_now().await;
        let ChatUpdate::Approval(id, name, _) = receiver.recv().unwrap() else {
            panic!("expected approval")
        };
        assert_eq!(name, "write_file");
        pending
            .lock()
            .unwrap()
            .remove(&id)
            .unwrap()
            .send(adk_rust::ToolConfirmationDecision::Approve)
            .unwrap();
        assert_eq!(
            task.await.unwrap(),
            adk_rust::ToolConfirmationDecision::Approve
        );
    }
}
