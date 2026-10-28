//! Small Win32 helpers: single instance, window visibility and capture exclusion.

use windows_sys::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HWND};
use windows_sys::Win32::System::Threading::{CreateMutexW, GetCurrentProcessId};
use windows_sys::Win32::UI::WindowsAndMessaging::*;

pub const WINDOW_TITLE: &str = "NextLine";

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Returns `false` if another instance already owns the mutex.
pub fn acquire_single_instance() -> bool {
    let name = wide("Local\\NextLine.SingleInstance");
    unsafe {
        let handle = CreateMutexW(std::ptr::null(), 0, name.as_ptr());
        // The handle is deliberately kept open for the life of the process.
        !handle.is_null() && GetLastError() != ERROR_ALREADY_EXISTS
    }
}

/// Finds the top-level window with our title. When `own_process` is set only this process matches.
pub fn find_main_window(own_process: bool) -> Option<HWND> {
    let title = wide(WINDOW_TITLE);
    let mut found: HWND = std::ptr::null_mut();
    unsafe {
        loop {
            found = FindWindowExW(std::ptr::null_mut(), found, std::ptr::null(), title.as_ptr());
            if found.is_null() {
                return None;
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(found, &mut pid);
            if own_process == (pid == GetCurrentProcessId()) {
                return Some(found);
            }
        }
    }
}

pub fn show_window(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        ShowWindow(hwnd, SW_RESTORE);
        SetForegroundWindow(hwnd);
    }
}

pub fn hide_window(hwnd: HWND) {
    unsafe {
        ShowWindow(hwnd, SW_HIDE);
    }
}

/// Hides the window from screen sharing and recording (Windows 10 2004+). The window still shows locally.
pub fn set_capture_excluded(hwnd: HWND, excluded: bool) {
    const WDA_NONE: u32 = 0x0;
    const WDA_EXCLUDEFROMCAPTURE: u32 = 0x11;
    unsafe {
        SetWindowDisplayAffinity(hwnd, if excluded { WDA_EXCLUDEFROMCAPTURE } else { WDA_NONE });
    }
}

/// Wakes the first instance when the user launches the app a second time.
pub fn focus_existing_instance() {
    if let Some(hwnd) = find_main_window(false) {
        show_window(hwnd);
    }
}
