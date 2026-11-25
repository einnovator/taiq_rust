//! Native, per-conversation terminal transport. The UI owns only its presentation state.
use anyhow::{Context, Result};
use portable_pty::{CommandBuilder, MasterPty, PtySize, native_pty_system};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Mutex};

pub struct TerminalSession {
    pub cwd: PathBuf,
    pub shell: String,
    master: Box<dyn MasterPty + Send>,
    child: Box<dyn portable_pty::Child + Send + Sync>,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
    output: Receiver<String>,
}

impl TerminalSession {
    pub fn open(cwd: &Path, columns: u16, rows: u16) -> Result<Self> {
        let cwd = cwd
            .canonicalize()
            .context("resolve terminal working directory")?;
        let shell = if cfg!(windows) {
            "powershell.exe".to_owned()
        } else {
            std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".to_owned())
        };
        let pair = native_pty_system().openpty(PtySize {
            rows: rows.max(1),
            cols: columns.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })?;
        let mut command = CommandBuilder::new(&shell);
        command.cwd(&cwd);
        if cfg!(windows) {
            command.arg("-NoLogo");
            command.arg("-NoProfile");
            command.arg("-NonInteractive");
        }
        let child = pair.slave.spawn_command(command)?;
        drop(pair.slave);
        let mut reader = pair.master.try_clone_reader()?;
        let writer = Arc::new(Mutex::new(pair.master.take_writer()?));
        let response_writer = Arc::clone(&writer);
        let (sender, output) = mpsc::channel();
        std::thread::spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(count) => {
                        if buffer[..count].windows(4).any(|part| part == b"\x1b[6n") {
                            // PowerShell requests cursor position from its terminal.
                            // Respond so its line editor can finish initializing.
                            if let Ok(mut writer) = response_writer.lock() {
                                let _ = writer.write_all(b"\x1b[1;1R");
                                let _ = writer.flush();
                            }
                        }
                        if sender
                            .send(String::from_utf8_lossy(&buffer[..count]).into_owned())
                            .is_err()
                        {
                            break;
                        }
                    }
                    Err(error) => {
                        let _ = sender.send(format!("\n[terminal error] {error}\n"));
                        break;
                    }
                }
            }
        });
        Ok(Self {
            cwd,
            shell,
            master: pair.master,
            child,
            writer,
            output,
        })
    }

    pub fn write_command(&mut self, command: &str) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(command.as_bytes())?;
        writer.write_all(if cfg!(windows) { b"\r" } else { b"\n" })?;
        writer.flush()?;
        Ok(())
    }

    pub fn interrupt(&mut self) -> Result<()> {
        let mut writer = self.writer.lock().unwrap();
        writer.write_all(&[3])?;
        writer.flush()?;
        Ok(())
    }

    pub fn resize(&self, columns: u16, rows: u16) -> Result<()> {
        self.master.resize(PtySize {
            rows: rows.max(1),
            cols: columns.max(1),
            pixel_width: 0,
            pixel_height: 0,
        })?;
        Ok(())
    }

    pub fn drain(&mut self) -> String {
        let mut text = String::new();
        while let Ok(chunk) = self.output.try_recv() {
            text.push_str(&chunk);
        }
        text
    }

    pub fn exited(&mut self) -> Result<bool> {
        Ok(self.child.try_wait()?.is_some())
    }
}

impl Drop for TerminalSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

/// Keep readable text in native widgets until a full ANSI terminal canvas is available.
pub fn plain_output(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            match chars.next() {
                Some('[') => {
                    for next in chars.by_ref() {
                        if ('@'..='~').contains(&next) {
                            break;
                        }
                    }
                }
                Some(']') => {
                    for next in chars.by_ref() {
                        if next == '\x07' {
                            break;
                        }
                    }
                }
                _ => {}
            }
        } else if ch == '\r' {
            if chars.peek() != Some(&'\n') {
                output.push('\n');
            }
        } else if ch != '\x08' {
            output.push(ch);
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::plain_output;
    #[test]
    fn removes_control_sequences_from_terminal_preview() {
        assert_eq!(plain_output("\x1b[31mred\x1b[0m\r\nnext"), "red\nnext");
    }

    #[cfg(windows)]
    #[test]
    fn native_terminal_runs_in_selected_directory() {
        let cwd = std::env::current_dir().unwrap();
        let mut terminal = super::TerminalSession::open(&cwd, 80, 24).unwrap();
        terminal
            .write_command("Write-Output ('TAIQ_CWD=' + (Get-Location).Path)")
            .unwrap();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
        let mut output = String::new();
        while std::time::Instant::now() < deadline {
            output.push_str(&terminal.drain());
            if output
                .lines()
                .filter_map(|line| line.split_once("TAIQ_CWD=").map(|(_, path)| path))
                .any(|path| path.trim_end().ends_with(&cwd.display().to_string()))
            {
                return;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        panic!("terminal did not report the selected directory: {output}");
    }
}
