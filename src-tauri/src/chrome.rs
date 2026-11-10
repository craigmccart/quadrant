//! Native title bar colours. We keep the real Windows title bar (so snap
//! layouts, dragging and resizing all behave) and just tint it to match the
//! app, which Windows 11 allows through DWM.

use tauri::WebviewWindow;
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::{
    DwmSetWindowAttribute, DWMWA_BORDER_COLOR, DWMWA_CAPTION_COLOR, DWMWA_TEXT_COLOR,
    DWMWINDOWATTRIBUTE,
};

/// `0x00BBGGRR`, the byte order Win32 wants.
const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((b as u32) << 16 | (g as u32) << 8 | r as u32)
}

struct Chrome {
    caption: COLORREF,
    text: COLORREF,
    border: COLORREF,
}

// Keep these in step with --bg, --fg and --border in ui/css/tokens.css.
const DARK: Chrome =
    Chrome { caption: rgb(19, 18, 16), text: rgb(236, 231, 222), border: rgb(48, 45, 40) };
const LIGHT: Chrome =
    Chrome { caption: rgb(246, 243, 238), text: rgb(28, 25, 21), border: rgb(214, 207, 195) };

fn set(hwnd: HWND, attribute: DWMWINDOWATTRIBUTE, colour: COLORREF) {
    // Fails harmlessly on Windows 10, which has no caption colour support.
    unsafe {
        let _ = DwmSetWindowAttribute(
            hwnd,
            attribute,
            &colour as *const COLORREF as *const _,
            std::mem::size_of::<COLORREF>() as u32,
        );
    }
}

/// Tint the title bar for "dark" or "light". Anything else is treated as dark.
pub fn apply(window: &WebviewWindow, theme: &str) {
    let light = theme == "light";
    let _ = window.set_theme(Some(if light { tauri::Theme::Light } else { tauri::Theme::Dark }));

    let Ok(handle) = window.hwnd() else { return };
    // Tauri may link a different version of the `windows` crate, so rebuild
    // the handle from its raw pointer rather than passing the type across.
    let hwnd = HWND(handle.0 as *mut _);
    let chrome = if light { LIGHT } else { DARK };
    set(hwnd, DWMWA_CAPTION_COLOR, chrome.caption);
    set(hwnd, DWMWA_TEXT_COLOR, chrome.text);
    set(hwnd, DWMWA_BORDER_COLOR, chrome.border);
}
