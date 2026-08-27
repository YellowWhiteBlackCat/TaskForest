//! Minimal Win32 message pump for the tray host thread.
//!
//! The `tray-icon` crate creates a hidden window and registers it with
//! `Shell_NotifyIconW`; the icon only receives its `WM_USER_TRAYICON`
//! callbacks while some loop pumps that window's messages. This module is
//! that loop: a bounded, non-blocking `PeekMessageW` drain with no pointer,
//! handle, or buffer crossing the public API.

use super::WindowsApiError;

/// Maximum messages dispatched in a single pump call. Bounds the work a host
/// thread performs per iteration and prevents a flood from starving the
/// application loop.
pub const MAX_PUMPED_MESSAGES_PER_CALL: u32 = 64;

/// Pump pending window messages for the calling thread (non-blocking).
///
/// Returns the number of messages dispatched. Messages are retrieved with
/// `PM_REMOVE` and dispatched through `TranslateMessage`/`DispatchMessageW`;
/// `WM_QUIT` is removed but never dispatched.
#[must_use = "the pump result is the dispatcher's progress signal"]
pub fn pump_pending_messages() -> Result<u32, WindowsApiError> {
    #[cfg(windows)]
    {
        pump_pending_messages_windows()
    }
    #[cfg(not(windows))]
    {
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn pump_pending_messages_windows() -> Result<u32, WindowsApiError> {
    use windows::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, PM_REMOVE, PeekMessageW, TranslateMessage, WM_QUIT,
    };

    let mut dispatched = 0u32;
    loop {
        if dispatched >= MAX_PUMPED_MESSAGES_PER_CALL {
            break;
        }
        let mut message = MSG::default();
        // SAFETY: `MSG` is zero-initialized; the None window handle asks
        // for every window and thread message of this thread; `PM_REMOVE`
        // retrieves and removes at most one message. The fixed cap keeps
        // the loop bounded.
        let has_message = unsafe { PeekMessageW(&mut message, None, 0, 0, PM_REMOVE) }.as_bool();
        if !has_message {
            break;
        }
        if message.message == WM_QUIT {
            // Removed by PM_REMOVE; never dispatched (WM_QUIT is not a
            // dispatchable message).
            continue;
        }
        // SAFETY: `message` is a valid `MSG` populated by PeekMessageW.
        let _ = unsafe { TranslateMessage(&message) };
        // SAFETY: `message` is a valid `MSG` populated by PeekMessageW.
        let _ = unsafe { DispatchMessageW(&message) };
        dispatched = dispatched.saturating_add(1);
    }
    Ok(dispatched)
}

#[cfg(all(test, windows))]
#[path = "../tests/headless/windows_api_msg_pump.rs"]
mod tests;
