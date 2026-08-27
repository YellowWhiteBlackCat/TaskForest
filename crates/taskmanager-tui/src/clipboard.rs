//! OSC 52 clipboard copy for the terminal frontend.
//!
//! A terminal has no in-process clipboard, and spawning a helper binary is
//! forbidden by the dependency firewall (see `ui/process_menu.rs`), so the
//! TUI copies through the terminal emulator's own clipboard: the OSC 52
//! escape sequence `ESC ] 52 ; <selection> ; <base64> BEL` instructs the
//! emulator to write the payload to the system clipboard. The `y` key on the
//! Applications page copies the selected process's `pid<TAB>name` line; the
//! payload is base64-encoded by hand (no dependency needed) and written to
//! the runtime's stdout sink.

use std::io::Write;

/// The byte sequence that introduces an OSC 52 clipboard write: ESC ] 52 ;
///
/// The emulator writes the base64 payload to the system clipboard (`c` =
/// clipboard selection); the payload is `pid<TAB>name` so pasting into a
/// terminal yields a copy-pasteable `pid\tname` pair.
const OSC52_PREFIX: &[u8] = b"\x1b]52;c;";
/// The OSC 52 terminator: the BEL character ends the sequence.
const OSC52_SUFFIX: &[u8] = b"\x07";

/// Base64-encode `input` per RFC 4648 without any dependency. The TUI is a
/// dependency-firewall boundary (no external crates beyond ratatui/crossterm),
/// so the encoder is a small pure function with table tests instead of a
/// crate.
#[must_use]
pub fn base64_encode(input: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() >= 2 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() >= 3 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

/// Write `payload` to the terminal emulator's system clipboard via OSC 52.
/// The bytes are written to `sink` (the runtime's stdout in production; a
/// test buffer elsewhere). Returns `io::Error` when the write fails — the
/// caller surfaces the failure through the status line, never a panic.
pub fn write_clipboard<W: Write>(sink: &mut W, payload: &str) -> std::io::Result<()> {
    sink.write_all(OSC52_PREFIX)?;
    sink.write_all(base64_encode(payload.as_bytes()).as_bytes())?;
    sink.write_all(OSC52_SUFFIX)?;
    sink.flush()
}

#[cfg(test)]
#[path = "../tests/gui/clipboard_tests.rs"]
mod tests;
