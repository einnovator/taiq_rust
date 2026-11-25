//! ForgeIQ's default web theme, shared by the native renderers.
//! Values mirror `src/static/css/forgeiq-theme.css` in the ForgeIQ project.

pub const FONT_FAMILY: &str = "Inter";
pub const FONT_BYTES: &[u8] = include_bytes!("fonts/Inter.ttf");
pub const BRAND_ICON: &[u8] = include_bytes!("icons/forgeiq.svg");

pub const BG: u32 = 0xf6f8fb;
pub const SURFACE: u32 = 0xffffff;
pub const SURFACE_MUTED: u32 = 0xf9fafb;
pub const BORDER: u32 = 0xe5e7eb;
pub const BORDER_SOFT: u32 = 0xeef2f7;
pub const TEXT: u32 = 0x111827;
pub const TEXT_MUTED: u32 = 0x6b7280;
pub const PRIMARY: u32 = 0x2563eb;
pub const PRIMARY_SOFT: u32 = 0xdbeafe;
pub const HEADER: u32 = 0x0f172a;
pub const HEADER_TEXT: u32 = 0xfffffe;
pub const DANGER: u32 = 0xef4444;

use std::sync::atomic::{AtomicBool, Ordering};

static DARK: AtomicBool = AtomicBool::new(false);

pub fn set_dark(value: bool) {
    DARK.store(value, Ordering::Relaxed);
}
pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

pub fn resolve(value: u32) -> u32 {
    if !is_dark() {
        return value;
    }
    match value {
        BG => 0x0b1120,
        SURFACE => 0x111827,
        SURFACE_MUTED => 0x182235,
        BORDER => 0x334155,
        BORDER_SOFT => 0x273449,
        TEXT => 0xf1f5f9,
        TEXT_MUTED => 0xaab8ca,
        PRIMARY => 0x93c5fd,
        PRIMARY_SOFT => 0x1e3a5f,
        HEADER => 0x060d1b,
        HEADER_TEXT => 0xf8fafc,
        DANGER => 0xf87171,
        _ => value,
    }
}
