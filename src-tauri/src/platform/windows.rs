//! Windows window behaviour (§6.3).

use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetWindowLongPtrW, SetWindowLongPtrW, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
};

/// Take Clawd out of Alt-Tab.
///
/// Tauri's `always_on_top` already applies `WS_EX_TOPMOST`, and `skip_taskbar`
/// removes the taskbar button — but it does that through `ITaskbarList`, which
/// leaves the window in the Alt-Tab list. `WS_EX_TOOLWINDOW` is what takes
/// that slot back. A desktop pet that occupies an Alt-Tab slot becomes
/// irritating within an hour.
pub fn configure(window: &WebviewWindow) {
    let Ok(hwnd) = window.hwnd() else {
        return;
    };
    let hwnd = hwnd.0 as HWND;

    // SAFETY: a live HWND owned by this window, called on the main thread.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(hwnd, GWL_EXSTYLE, style | WS_EX_TOOLWINDOW as isize);
    }
}
