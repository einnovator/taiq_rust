use adk_rust::prelude::*;
use adk_rust::session::{CreateRequest, SessionService};
use anyhow::Result;
use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEventKind, KeyModifiers};
use crossterm::style::Print;
use crossterm::terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{execute, queue};
use futures::StreamExt;
use std::collections::HashMap;
use std::io::{Write, stdout};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

enum Update {
    Text(String),
    Done,
}

struct Screen;
impl Screen {
    fn enter() -> Result<Self> {
        terminal::enable_raw_mode()?;
        execute!(stdout(), EnterAlternateScreen, Hide)?;
        Ok(Self)
    }
}
impl Drop for Screen {
    fn drop(&mut self) {
        let _ = execute!(stdout(), Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

pub async fn run(agent: Arc<dyn Agent>, title: &str) -> Result<()> {
    let service: Arc<dyn SessionService> = Arc::new(InMemorySessionService::new());
    let session = service
        .create(CreateRequest {
            app_name: "taiq".into(),
            user_id: "terminal".into(),
            session_id: None,
            state: HashMap::new(),
        })
        .await?;
    let session_id = session.id().to_string();
    let runner = Arc::new(
        Runner::builder()
            .app_name("taiq")
            .agent(agent)
            .session_service(service)
            .build()?,
    );
    let _screen = Screen::enter()?;
    let mut history: Vec<(String, String)> = Vec::new();
    let mut input = String::new();
    let mut busy = false;
    let mut dirty = true;
    let (tx, mut rx) = mpsc::unbounded_channel::<Update>();
    loop {
        while let Ok(update) = rx.try_recv() {
            dirty = true;
            match update {
                Update::Text(chunk) => {
                    if let Some((_, answer)) = history.last_mut() {
                        answer.push_str(&chunk);
                    }
                }
                Update::Done => busy = false,
            }
        }
        if dirty {
            draw(title, &history, &input, busy)?;
            dirty = false;
        }
        if event::poll(Duration::from_millis(50))? {
            let terminal_event = event::read()?;
            if matches!(terminal_event, Event::Resize(_, _)) {
                dirty = true;
            }
            if let Event::Key(key) = terminal_event {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Esc => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    KeyCode::Enter if !busy && !input.trim().is_empty() => {
                        dirty = true;
                        let prompt = std::mem::take(&mut input);
                        history.push(("You".into(), prompt.clone()));
                        history.push((title.into(), String::new()));
                        busy = true;
                        let runner = runner.clone();
                        let session_id = session_id.clone();
                        let tx = tx.clone();
                        tokio::spawn(async move {
                            let result = send_turn(runner, &session_id, &prompt, &tx).await;
                            if let Err(error) = result {
                                let _ = tx.send(Update::Text(format!("\n[error] {error}")));
                            }
                            let _ = tx.send(Update::Done);
                        });
                    }
                    KeyCode::Backspace => {
                        input.pop();
                        dirty = true;
                    }
                    KeyCode::Char(c) if !key.modifiers.contains(KeyModifiers::CONTROL) => {
                        input.push(c);
                        dirty = true;
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

async fn send_turn(
    runner: Arc<Runner>,
    session_id: &str,
    prompt: &str,
    tx: &mpsc::UnboundedSender<Update>,
) -> Result<()> {
    let content = Content::new("user").with_text(prompt);
    let mut stream = runner.run_str("terminal", session_id, content).await?;
    let mut deltas = adk_rust::EventTextDeltas::default();
    while let Some(event) = stream.next().await {
        let event = event?;
        let event = deltas.push(&event);
        if let Some(content) = event.content() {
            for part in &content.parts {
                match part {
                    Part::Text { text } if !text.is_empty() => {
                        let _ = tx.send(Update::Text(text.clone()));
                    }
                    Part::FunctionCall { name, .. } => {
                        let _ = tx.send(Update::Text(format!("\n[tool: {name}]\n")));
                    }
                    _ => {}
                }
            }
        }
    }
    Ok(())
}

fn draw(title: &str, history: &[(String, String)], input: &str, busy: bool) -> Result<()> {
    let (width, height) = terminal::size()?;
    let usable = width.saturating_sub(1) as usize;
    let mut lines = Vec::new();
    for (speaker, message) in history {
        lines.push(format!("{speaker}:"));
        for line in message.lines() {
            if line.is_empty() {
                lines.push(String::new());
            } else {
                let chars: Vec<char> = line.chars().collect();
                for chunk in chars.chunks(usable.max(1)) {
                    lines.push(chunk.iter().collect());
                }
            }
        }
        lines.push(String::new());
    }
    let transcript_height = height.saturating_sub(4) as usize;
    let start = lines.len().saturating_sub(transcript_height);
    let mut out = stdout();
    queue!(
        out,
        MoveTo(0, 0),
        Clear(ClearType::All),
        Print(format!("taiq | {title} | Esc to quit"))
    )?;
    for (row, line) in lines[start..].iter().enumerate() {
        queue!(out, MoveTo(0, row as u16 + 2), Print(line))?;
    }
    let status = if busy { "thinking..." } else { "Enter to send" };
    queue!(
        out,
        MoveTo(0, height.saturating_sub(2)),
        Print(status),
        MoveTo(0, height.saturating_sub(1)),
        Print(format!("> {input}"))
    )?;
    out.flush()?;
    Ok(())
}
