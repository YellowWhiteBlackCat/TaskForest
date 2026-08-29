//! AF_PACKET net-launcher invocation (ADR-024/025) — the fd-returning analog of
//! the perf helper in `polkit`. The launcher opens the AF_PACKET socket with
//! CAP_NET_RAW (granted by the OS-native prompt) and hands the fd back over
//! SCM_RIGHTS; the unprivileged app then runs the capture loop on it.
//!
//! Isolated in its own module (mirroring `json_reader`) so `polkit` stays under
//! the workspace file-line budget, and to keep the net-launcher crossing —
//! abstraction, driver, and the Linux `pkexec` implementation — in one place.
//!
//! ## Platform abstraction + implementation isolation
//!
//! The outcome / trait / generic driver are cross-platform: [`NetLaunchHandle`]
//! aliases the owned-handle type per platform (Unix `OwnedFd` / Windows
//! `OwnedSocket`), so the API compiles on every target. Only the Linux
//! `pkexec` + SCM_RIGHTS driver ([`PkexecNetLauncher`]) is cfg-gated; off-Linux
//! [`invoke_net_launcher`] returns `Unavailable` (`Success` is never produced
//! there). This mirrors `invoke_perf_helper`, which stays cross-platform because
//! its outcome carries plain data rather than a handle.
//!
//! ## Handoff hardening (the safe side of ADR-025)
//!
//! The return channel is a Linux ABSTRACT-namespace Unix socket with a
//! 16-byte `/dev/urandom` name — never a filesystem path, so there is no
//! world-connectable `/tmp` file to seize, chmod, or leak across retries.
//! After `accept` the kernel-side `SO_PEERCRED` credentials
//! (`taskmanager_fd_bridge::peer_credentials`) must show `uid == 0` (the
//! pkexec'd launcher) before [`recv_fd`] is ever called: a local unprivileged
//! process that guesses the abstract name is disconnected, not indulged.
//! Random name + credential gate are independent defenses (either alone
//! closes the injection race).

#![forbid(unsafe_code)]

use std::io;

use crate::EscalationDenialReason;

/// The absolute install path of the net-launcher binary. MUST match the
/// `org.freedesktop.policykit.exec.path` annotation in
/// `polkit/io.github.YellowWhiteBlackCat.TaskForest.net-launcher.policy.in`
/// byte-for-byte — polkit resolves the action by that path.
#[cfg(target_os = "linux")]
pub(crate) const NET_LAUNCHER_PATH: &str = "/usr/libexec/taskforest-net-launcher";

// The platform-agnostic owned handle a successful net-launch hands back. Unix
// models an AF_PACKET socket as an owned file descriptor; Windows would model
// an equivalent as an owned socket (no Windows net-launcher exists today, so
// `Success` is unreachable there — the non-Linux impl returns `Unavailable`).
// Aliasing the handle keeps the outcome / trait / driver below cross-platform;
// only the Linux `pkexec` driver that actually produces one is cfg-gated.
#[cfg(unix)]
pub type NetLaunchHandle = std::os::fd::OwnedFd;
#[cfg(windows)]
pub type NetLaunchHandle = std::os::windows::io::OwnedSocket;

/// The typed result of one net-launcher invocation: either the received
/// capture handle ([`NetLauncherOutcome::Success`]) or why it was unavailable.
#[derive(Debug)]
pub enum NetLauncherOutcome {
    /// The launcher handed over a bound capture handle; the caller owns it.
    Success(NetLaunchHandle),
    /// The fd could not be obtained (the user declined, the launcher is missing,
    /// or the open/handoff failed). Honest — never a fabricated fd.
    Unavailable {
        reason: EscalationDenialReason,
        detail: String,
    },
}

/// Side-effect-free process seam for the net-launcher. Production runs `pkexec`
/// plus the SCM_RIGHTS handoff; tests return a canned handle or a synthetic
/// error. Cross-platform — the Linux `pkexec` driver is the only real impl;
/// off-Linux `invoke_net_launcher` returns `Unavailable` without one.
pub trait NetLauncherProcess {
    /// Provision the capture socket via the privileged launcher and return the
    /// owned handle.
    fn obtain_fd(&self, iface_index: u32) -> io::Result<NetLaunchHandle>;
}

/// Production driver: bind a randomly named abstract Unix socket, `pkexec` the
/// launcher with the (hex) abstract name + interface index, accept only a
/// uid-0 peer (`SO_PEERCRED`), receive the fd via SCM_RIGHTS, ACK, and return
/// it. Linux-only — polkit/pkexec do not exist on macOS/Windows.
///
/// Lifecycle: one RAII guard (`PkexecChild`) owns the pkexec child for the
/// whole crossing, so EVERY exit — accept timeout, credential rejection,
/// recv failure, ACK failure — reaps the (potentially still root) launcher
/// bounded, instead of leaving it blocked forever on an ACK that never comes.
///
/// **On-box-unverified:** the live pkexec prompt + CAP_NET_RAW + SCM_RIGHTS
/// handoff cannot be exercised headless (no active polkit session, no
/// capability). The fd-passing + credential primitives are unit-tested in
/// `taskmanager-fd-bridge`; the name/credential/guard orchestration is tested
/// in this crate's headless suite; the full chain is verified only on-box.
#[cfg(target_os = "linux")]
#[derive(Debug, Clone, Copy, Default)]
pub struct PkexecNetLauncher;

#[cfg(target_os = "linux")]
impl PkexecNetLauncher {
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

#[cfg(target_os = "linux")]
impl NetLauncherProcess for PkexecNetLauncher {
    fn obtain_fd(&self, iface_index: u32) -> io::Result<NetLaunchHandle> {
        use std::time::{Duration, Instant};

        // pkexec sanitizes inherited fds, so the address is passed as a CLI arg
        // (hex — NUL-free and printable) and the launcher connects back to this
        // throwaway abstract socket. No file is ever created.
        let handoff = HandoffName::generate()?;
        let listener = bind_handoff_listener(&handoff)?;
        listener.set_nonblocking(true)?;

        // Every return below goes through the guard's bounded reap.
        let mut child = PkexecChild::spawn(&handoff.hex_name(), iface_index)?;

        // Poll for the launcher's connection, admitting ONLY a uid-0 peer,
        // bailing if it exits first (an open failure → it never connects) or
        // the deadline passes (the prompt was declined / never answered).
        let stream = accept_privileged_peer(
            &listener,
            Instant::now() + Duration::from_secs(ACCEPT_DEADLINE_SECS),
            || child.has_exited(),
        )?;
        stream.set_nonblocking(false)?;
        // A recv timeout so a launcher that connected but failed to send surfaces.
        stream.set_read_timeout(Some(Duration::from_secs(RECV_TIMEOUT_SECS)))?;
        let fd = taskmanager_fd_bridge::recv_fd(&stream)?;
        // ACK so the launcher may exit — closes the close-before-transfer race
        // (the kernel has now duplicated the fd into our table). Best-effort:
        // a failed ACK write never invalidates an fd the kernel already gave us.
        use std::io::Write;
        let _ = (&stream).write_all(&[0u8]);
        // Bounded final reap (the launcher exits right after reading the ACK);
        // the exit status is not second-guessed — the fd is the result.
        let _ = child.wait_bounded(Duration::from_secs(FINAL_REAP_PATIENCE_SECS))?;
        Ok(fd)
    }
}

#[cfg(target_os = "linux")]
/// Seconds the driver waits for the pkexec'd launcher to connect back (the
/// OS-native prompt can legitimately take tens of seconds to answer).
const ACCEPT_DEADLINE_SECS: u64 = 30;

#[cfg(target_os = "linux")]
/// Seconds `recv_fd` may block once a privileged peer is connected.
const RECV_TIMEOUT_SECS: u64 = 5;

#[cfg(target_os = "linux")]
/// Seconds the guard polls a child before killing it (used for the final ACK
/// wait and for every error-path drop).
const FINAL_REAP_PATIENCE_SECS: u64 = 5;

#[cfg(target_os = "linux")]
mod launcher_internals {
    use super::NET_LAUNCHER_PATH;
    use std::io;
    use std::os::linux::net::SocketAddrExt;
    use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
    use std::time::{Duration, Instant};

    /// Randomly named abstract handoff socket (the safe half of the ADR-025
    /// hardening): a `\0`-prefixed kernel-abstract name built from a fixed
    /// prefix + 16 bytes of `/dev/urandom`. There is no filesystem path to
    /// create, `chmod`, or remove — the name vanishes with the listener, so
    /// repeated invocations can never accumulate `/tmp` residue, and guessing
    /// the name does not bypass the `SO_PEERCRED` uid gate.
    pub(super) struct HandoffName {
        bytes: Vec<u8>,
    }

    /// Discriminating prefix for TaskForest handoff names (observability in
    /// `ss`/`netstat` output; carries no security meaning).
    const HANDOFF_PREFIX: &[u8] = b"tm-netl-";

    /// Entropy of the random suffix — a UUID's worth of unguessability.
    const HANDOFF_RANDOM_LEN: usize = 16;

    impl HandoffName {
        pub(super) fn generate() -> io::Result<Self> {
            use std::io::Read;
            let mut random = [0u8; HANDOFF_RANDOM_LEN];
            let mut source = std::fs::File::open("/dev/urandom")?;
            source.read_exact(&mut random)?;
            let mut bytes = Vec::with_capacity(HANDOFF_PREFIX.len() + HANDOFF_RANDOM_LEN);
            bytes.extend_from_slice(HANDOFF_PREFIX);
            bytes.extend_from_slice(&random);
            Ok(Self { bytes })
        }

        pub(super) fn as_bytes(&self) -> &[u8] {
            &self.bytes
        }

        /// CLI-argument encoding: hex keeps the name NUL-free and printable
        /// for the pkexec argv; the launcher hex-decodes it before
        /// `connect_addr`.
        pub(super) fn hex_name(&self) -> String {
            let mut hex = String::with_capacity(self.bytes.len() * 2);
            for byte in &self.bytes {
                hex.push_str(&format!("{byte:02x}"));
            }
            hex
        }
    }

    /// RAII reap guard for the pkexec child: the privileged launcher blocks
    /// reading its one-byte ACK after sending the fd, so an app-side error
    /// return that merely dropped the `Child` would leave the ROOT process
    /// hanging forever. Every path — error returns via `?`, timeouts, success —
    /// ends in a bounded `try_wait` poll, then kill, then wait. The guard
    /// exists per-spawn and is not cloneable/shared.
    pub(super) struct PkexecChild {
        child: std::process::Child,
        done: bool,
    }

    impl PkexecChild {
        /// Wrap an already-spawned child in the guard (test seam: the guard's
        /// reap/kill behavior is testable against any benign child, without
        /// ever launching pkexec from a test).
        #[cfg(all(test, target_os = "linux"))]
        pub(super) fn wrapping(child: std::process::Child) -> Self {
            Self { child, done: false }
        }

        /// `pkexec` the net-launcher with the fixed argument vocabulary
        /// (abstract name + interface index). No shell, no env, no extra fds.
        pub(super) fn spawn(hex_name: &str, iface_index: u32) -> io::Result<Self> {
            let child = std::process::Command::new("pkexec")
                .arg(NET_LAUNCHER_PATH)
                .arg(hex_name)
                .arg(iface_index.to_string())
                .spawn()?;
            Ok(Self { child, done: false })
        }

        /// Bounded reap: poll `try_wait` until `patience` elapses, then kill
        /// and wait (a SIGKILLed child cannot outlive the wait). Marks the
        /// child reaped so `Drop` will not repeat the cycle.
        pub(super) fn wait_bounded(
            &mut self,
            patience: Duration,
        ) -> io::Result<std::process::ExitStatus> {
            let deadline = Instant::now() + patience;
            loop {
                if let Some(status) = self.child.try_wait()? {
                    self.done = true;
                    return Ok(status);
                }
                if Instant::now() >= deadline {
                    let _ = self.child.kill();
                    // Reap synchronously — a SIGKILLed child cannot outlive
                    // this wait; the status is irrelevant on the timeout path.
                    self.child.wait()?;
                    self.done = true;
                    return Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "net-launcher did not exit in time after the handoff; killed",
                    ));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
        }

        /// Whether the child has already exited (non-blocking; `Ok(false)` is
        /// "still running", an `Err` is a real wait failure).
        pub(super) fn has_exited(&mut self) -> io::Result<bool> {
            Ok(self.child.try_wait()?.is_some())
        }
    }

    impl Drop for PkexecChild {
        fn drop(&mut self) {
            if !self.done {
                let _ = self.wait_bounded(Duration::from_secs(super::FINAL_REAP_PATIENCE_SECS));
            }
        }
    }

    /// Accept the pkexec'd launcher's connection, gated on the kernel's
    /// `SO_PEERCRED` credentials: only `uid == 0` is admitted. Any other
    /// connection is disconnected immediately and recorded — an unprivileged
    /// local process that raced onto the abstract name never reaches
    /// `recv_fd`. The loop keeps waiting for the real launcher until
    /// `deadline` passes or `launcher_exited` reports the child gone, so a
    /// stray connect cannot permanently deny the handoff.
    pub(super) fn accept_privileged_peer(
        listener: &UnixListener,
        deadline: Instant,
        mut launcher_exited: impl FnMut() -> io::Result<bool>,
    ) -> io::Result<UnixStream> {
        let mut rejected_uids: Vec<u32> = Vec::new();
        loop {
            match listener.accept() {
                Ok((stream, _)) => {
                    let credentials = taskmanager_fd_bridge::peer_credentials(&stream)?;
                    if credentials.uid == 0 {
                        return Ok(stream);
                    }
                    rejected_uids.push(credentials.uid);
                    // Disconnect the unprivileged peer right away; the typed
                    // record surfaces below if no launcher ever arrives.
                    drop(stream);
                }
                Err(ref error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(ref error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            format!(
                                "net-launcher did not connect in time (rejected {} unprivileged \
                                 peer connection(s), uids {rejected_uids:?})",
                                rejected_uids.len()
                            ),
                        ));
                    }
                    if launcher_exited()? {
                        return Err(io::Error::new(
                            io::ErrorKind::ConnectionRefused,
                            "net-launcher exited before connecting (open failure or denial)",
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                Err(error) => return Err(error),
            }
        }
    }

    /// Bind the listener for a handoff name — the production `bind` step,
    /// split out so tests exercise the exact same abstract-namespace path.
    pub(super) fn bind_handoff_listener(name: &HandoffName) -> io::Result<UnixListener> {
        let addr = SocketAddr::from_abstract_name(name.as_bytes()).map_err(|error| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                format!("abstract name rejected: {error}"),
            )
        })?;
        UnixListener::bind_addr(&addr)
    }
}

#[cfg(target_os = "linux")]
use launcher_internals::{HandoffName, PkexecChild, accept_privileged_peer, bind_handoff_listener};

/// Drive one net-launcher invocation through `process` and map the raw result to
/// a typed [`NetLauncherOutcome`]. A received fd is
/// [`NetLauncherOutcome::Success`]; any error is
/// [`NetLauncherOutcome::Unavailable`] (with `PermissionDenied` only when the
/// launcher reported a permission failure; otherwise `HelperUnavailable`). The precise launcher
/// exit-code classification is deferred to on-box refinement — this maps
/// honestly without inventing a fd.
///
/// `P` is `?Sized` so the runtime object-safe lane (`Box<dyn
/// NetLauncherProcess + Send>` held by the capability provider) can drive this
/// exact mapping instead of mirroring it.
pub fn invoke_net_launcher_with<P: NetLauncherProcess + ?Sized>(
    process: &P,
    iface_index: u32,
) -> NetLauncherOutcome {
    match process.obtain_fd(iface_index) {
        Ok(fd) => NetLauncherOutcome::Success(fd),
        Err(error) => {
            let reason = match error.kind() {
                io::ErrorKind::PermissionDenied => EscalationDenialReason::PermissionDenied,
                _ => EscalationDenialReason::HelperUnavailable,
            };
            NetLauncherOutcome::Unavailable {
                reason,
                detail: format!("net-launcher invocation failed: {error}"),
            }
        }
    }
}

/// Run the net-launcher end-to-end via the production `pkexec` driver.
#[cfg(target_os = "linux")]
pub fn invoke_net_launcher(iface_index: u32) -> NetLauncherOutcome {
    invoke_net_launcher_with(&PkexecNetLauncher::new(), iface_index)
}

/// Off-Linux: per-process-net capture is Linux-only (AF_PACKET + pkexec), so
/// the feature is honestly unreachable — return `Unavailable` without invoking
/// anything. Mirrors `invoke_perf_helper`; the outcome stays cross-platform
/// because the handle type is abstracted (`Success` is never produced here).
#[cfg(not(target_os = "linux"))]
pub fn invoke_net_launcher(_iface_index: u32) -> NetLauncherOutcome {
    NetLauncherOutcome::Unavailable {
        reason: EscalationDenialReason::Unsupported,
        detail: "pkexec/polkit per-feature escalation is Linux-only".to_owned(),
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "../../tests/headless/escalation_polkit_net_launcher.rs"]
mod tests;
