//! Launch Windows voice typing into the focused chat composer.
use anyhow::Result;

#[cfg(windows)]
pub fn start() -> Result<()> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, SendInput, VK_H, VK_LWIN,
    };
    fn key(code: u16, up: bool) -> INPUT {
        INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: code,
                    dwFlags: if up { KEYEVENTF_KEYUP } else { 0 },
                    ..Default::default()
                },
            },
        }
    }
    // The UI focuses the composer first; Windows then types its transcript there.
    std::thread::sleep(std::time::Duration::from_millis(120));
    let sequence = [
        key(VK_LWIN, false),
        key(VK_H, false),
        key(VK_H, true),
        key(VK_LWIN, true),
    ];
    let sent = unsafe {
        SendInput(
            sequence.len() as u32,
            sequence.as_ptr(),
            std::mem::size_of::<INPUT>() as i32,
        )
    };
    anyhow::ensure!(
        sent == sequence.len() as u32,
        "could not start Windows voice typing"
    );
    Ok(())
}

#[cfg(not(windows))]
pub fn start() -> Result<()> {
    anyhow::bail!("NotImplemented: voice typing on this platform")
}
