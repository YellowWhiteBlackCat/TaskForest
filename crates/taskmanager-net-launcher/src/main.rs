//! `taskmanager-net-launcher` — the privileged launcher for per-process network
//! accounting.
//!
//! Invoked through the OS-native escalation prompt (polkit + `pkexec` on Linux,
//! per ADR-023 and `docs/PERMISSION_MODEL.md` Boundary 2), it performs exactly
//! ONE privileged operation: open an `AF_PACKET` raw socket bound to one
//! interface (needs `CAP_NET_RAW`), then hand the fd to the unprivileged app
//! over a Unix domain socket via `SCM_RIGHTS` and exit. The app then runs the
//! capture loop on the inherited fd without ever holding `CAP_NET_RAW` — only
//! the kernel object survives, owned by the unprivileged process.
//!
//! It reaches the audited boundary crates ONLY through their safe APIs:
//! `taskmanager_afpacket::PacketSource::open_packet_fd` (ADR-024) to obtain
//! the passable `OwnedFd`, and `taskmanager_fd_bridge::send_fd` (ADR-025) to
//! transfer it. (Plain code spans, not intra-doc links: the boundary crates
//! are empty on non-Linux targets where this file's doc build still runs.)
//! This binary writes no `unsafe`.
//!
//! Args: `<abstract-socket-name-hex> <iface-index>`. The unprivileged app
//! binds a randomly named ABSTRACT-namespace Unix socket (hex-encoded here —
//! argv is NUL-terminated C strings), accepts the connection only after the
//! kernel-side `SO_PEERCRED` check confirms a uid-0 peer, recv's the fd, and
//! writes a one-byte ACK so the launcher does not exit (and drop its fd
//! reference) before the kernel has duplicated the fd into the app's table —
//! closing the close-before-transfer race. The ACK read is BOUNDED: if the
//! app never answers (crashed mid-handoff), this process exits typed instead
//! of hanging forever holding the privileged fd.
//!
//! Honesty red line: a permission denial, an open failure, or a send failure
//! emits a typed ERROR envelope on stdout and a non-zero exit. The launcher
//! NEVER passes a bad or closed fd.
//!
//! Shared JSON contract (mirrors `taskmanager-privilege-helper`):
//! ```text
//! ERROR: {"status":"error","kind":"permission_denied"|"open_failed"|"send_failed"|"arg_error"|"connect_failed"|"ack_failed",
//!         "detail":"<string>"}
//! ```
//! Success is signaled by passing the fd, reading the ACK, and exiting 0 — no
//! stdout object (the fd IS the result).

#![forbid(unsafe_code)]

// This binary is the Linux half of the ADR-023/024/025 escalation chain
// (pkexec + AF_PACKET + SCM_RIGHTS) and has no non-Linux build; the stub main
// keeps the workspace compiling on Windows/macOS without any of the Unix
// seam's types.
#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!(
        "taskmanager-net-launcher is the Linux pkexec/AF_PACKET helper; \
         there is no build of it on this platform"
    );
}

#[cfg(target_os = "linux")]
use std::io::{self, Read, Write};
#[cfg(target_os = "linux")]
use std::os::linux::net::SocketAddrExt;
#[cfg(target_os = "linux")]
use std::os::unix::net::{SocketAddr, UnixStream};
#[cfg(target_os = "linux")]
use std::process::ExitCode;
#[cfg(target_os = "linux")]
use std::time::Duration;

#[cfg(target_os = "linux")]
use taskmanager_afpacket::PacketSource;
#[cfg(target_os = "linux")]
use taskmanager_fd_bridge::send_fd;

/// How long the ACK read may block: the receiver ACKs immediately after its
/// `recv_fd` returns, so this only fires when the app died mid-handoff — the
/// launcher then exits typed instead of hanging as a root process forever.
#[cfg(target_os = "linux")]
const ACK_TIMEOUT: Duration = Duration::from_secs(10);

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    let outcome = run();
    emit(outcome)
}

/// The typed outcome of one launch attempt. `Ok` carries no data — success is
/// the fd having been handed over + ACKed (the caller observes the fd on its
/// socket). `Err` carries the error kind for the typed envelope + exit code.
#[cfg(target_os = "linux")]
type Outcome = Result<(), LaunchError>;

#[cfg(target_os = "linux")]
#[derive(Debug)]
enum LaunchError {
    ArgError,
    ConnectFailed,
    PermissionDenied,
    OpenFailed,
    SendFailed,
    AckFailed,
}

#[cfg(target_os = "linux")]
impl LaunchError {
    const fn kind(&self) -> &'static str {
        match self {
            Self::ArgError => "arg_error",
            Self::ConnectFailed => "connect_failed",
            Self::PermissionDenied => "permission_denied",
            Self::OpenFailed => "open_failed",
            Self::SendFailed => "send_failed",
            Self::AckFailed => "ack_failed",
        }
    }

    const fn exit_code(&self) -> u8 {
        match self {
            Self::ArgError => 7,
            Self::ConnectFailed => 8,
            Self::PermissionDenied => 2,
            Self::OpenFailed => 4,
            Self::SendFailed => 6,
            Self::AckFailed => 9,
        }
    }
}

#[cfg(target_os = "linux")]
fn run() -> Outcome {
    let mut args = std::env::args().skip(1);
    let socket_name_hex = args.next().ok_or(LaunchError::ArgError)?;
    let iface_index: u32 = args
        .next()
        .ok_or(LaunchError::ArgError)
        .and_then(|raw| raw.parse().map_err(|_| LaunchError::ArgError))?;

    let name = decode_hex(&socket_name_hex).ok_or(LaunchError::ArgError)?;
    // The app binds a randomly named abstract-namespace socket; connect by
    // address (no filesystem path is ever involved).
    let addr = SocketAddr::from_abstract_name(&name).map_err(|_| LaunchError::ArgError)?;
    let mut stream = UnixStream::connect_addr(&addr).map_err(|_| LaunchError::ConnectFailed)?;

    let packet_fd = PacketSource::open_packet_fd(iface_index).map_err(|error| {
        // EPERM/EPERM → permission_denied (the host lacks CAP_NET_RAW even for
        // root, e.g. a restricted container); anything else → open_failed.
        if error.kind() == io::ErrorKind::PermissionDenied {
            LaunchError::PermissionDenied
        } else {
            LaunchError::OpenFailed
        }
    })?;

    send_fd(&stream, &packet_fd).map_err(|_| LaunchError::SendFailed)?;

    // Wait for the app's one-byte ACK so we do not exit (and drop our fd
    // reference) before the kernel has duplicated the fd into the app's table.
    // Bounded: a receiver that never answers gets a typed AckFailed exit, not
    // an immortal privileged process. (read_exact retries EINTR internally.)
    stream
        .set_read_timeout(Some(ACK_TIMEOUT))
        .map_err(|_| LaunchError::AckFailed)?;
    let mut ack = [0u8; 1];
    stream
        .read_exact(&mut ack)
        .map_err(|_| LaunchError::AckFailed)?;
    Ok(())
}

/// Decode the hex-encoded abstract-socket name handed over as the first CLI
/// argument. Any non-hex, odd-length, or non-UTF8-safe input is `None` →
/// `arg_error` (fail closed — never guess an address to connect to).
#[cfg(target_os = "linux")]
fn decode_hex(raw: &str) -> Option<Vec<u8>> {
    if raw.is_empty() || !raw.len().is_multiple_of(2) {
        return None;
    }
    let bytes = raw.as_bytes();
    (0..bytes.len())
        .step_by(2)
        .map(|index| {
            let pair = std::str::from_utf8(&bytes[index..index + 2]).ok()?;
            u8::from_str_radix(pair, 16).ok()
        })
        .collect()
}

/// Emit the typed ERROR envelope on stdout (success is silent — the fd was the
/// result) and map to the process exit code.
#[cfg(target_os = "linux")]
fn emit(outcome: Outcome) -> ExitCode {
    match outcome {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Hand-written JSON (no serde dep) — the kind set is fixed + simple.
            let detail = error_kind_detail(&error);
            let _ = writeln!(
                io::stdout(),
                r#"{{"status":"error","kind":"{}","detail":"{}"}}"#,
                error.kind(),
                detail.replace('\\', r"\\").replace('"', r#"\""#)
            );
            let _ = io::stdout().flush();
            ExitCode::from(error.exit_code())
        }
    }
}

#[cfg(target_os = "linux")]
fn error_kind_detail(error: &LaunchError) -> &'static str {
    match error {
        LaunchError::ArgError => {
            "usage: taskmanager-net-launcher <abstract-socket-name-hex> <iface-index>"
        }
        LaunchError::ConnectFailed => "could not connect the abstract handoff socket",
        LaunchError::PermissionDenied => "AF_PACKET open denied (CAP_NET_RAW absent)",
        LaunchError::OpenFailed => "AF_PACKET open failed",
        LaunchError::SendFailed => "SCM_RIGHTS send failed",
        LaunchError::AckFailed => "did not receive the receiver's ACK in time",
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../tests/headless/launcher_contract.rs"]
mod tests;
