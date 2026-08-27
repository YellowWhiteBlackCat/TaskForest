//! Fixed privileged setup helper for the Mission Center-compatible first-run
//! path.
//!
//! The helper is intentionally tiny and Linux-specific. It accepts exactly one
//! action (`install` or `revert`), writes/removes exactly one known udev rule,
//! and invokes only an absolute-path `udevadm` binary with fixed arguments.
//! It never interprets a caller-provided command or path.

#![forbid(unsafe_code)]
// The udev-rule surface (paths, content, conflict/io failure kinds) exists
// only behind the unix gates; on other targets this binary compiles down to
// the typed-unavailable entry, so the unix-only surface is dead code there by
// design.
#![cfg_attr(not(unix), allow(dead_code))]

// The fs/io/path/Command imports feed the unix-gated udev paths below; only
// ExitCode is consumed by the platform-neutral entry/error envelope.
#[cfg(unix)]
use std::fs::{self, OpenOptions};
#[cfg(unix)]
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::path::{Path, PathBuf};
use std::process::ExitCode;
#[cfg(unix)]
use std::process::{Child, Command, Stdio};
#[cfg(unix)]
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(unix)]
use std::sync::{Arc, Mutex};
#[cfg(unix)]
use std::thread;
#[cfg(unix)]
use std::time::{Duration, Instant};

const RULE_PATH: &str = "/etc/udev/rules.d/99-taskmanager.rules";
const RULE_CONTENT: &str = include_str!("../../../packaging/linux/99-taskmanager.rules");
const EXIT_INVALID_ARGUMENT: u8 = 64;
const EXIT_MISSING_DEPENDENCY: u8 = 69;
const EXIT_IO: u8 = 74;
const EXIT_CONFLICT: u8 = 75;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Operation {
    Install,
    Revert,
}

impl Operation {
    fn parse(argument: Option<&str>) -> Result<Self, HelperError> {
        match argument {
            Some("install") => Ok(Self::Install),
            Some("revert") => Ok(Self::Revert),
            Some(other) => Err(HelperError::InvalidArgument(format!(
                "unsupported setup action: {other}"
            ))),
            None => Err(HelperError::InvalidArgument(
                "one setup action is required".to_owned(),
            )),
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::Install => "install",
            Self::Revert => "revert",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
enum HelperError {
    InvalidArgument(String),
    MissingDependency(String),
    Conflict(String),
    Io(String),
}

impl HelperError {
    const fn exit_code(&self) -> u8 {
        match self {
            Self::InvalidArgument(_) => EXIT_INVALID_ARGUMENT,
            Self::MissingDependency(_) => EXIT_MISSING_DEPENDENCY,
            Self::Conflict(_) => EXIT_CONFLICT,
            Self::Io(_) => EXIT_IO,
        }
    }

    const fn kind(&self) -> &'static str {
        match self {
            Self::InvalidArgument(_) => "invalid_argument",
            Self::MissingDependency(_) => "missing_dependency",
            Self::Conflict(_) => "conflict",
            Self::Io(_) => "io_error",
        }
    }

    fn detail(&self) -> &str {
        match self {
            Self::InvalidArgument(detail)
            | Self::MissingDependency(detail)
            | Self::Conflict(detail)
            | Self::Io(detail) => detail,
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let operation = match Operation::parse(args.get(1).map(String::as_str)) {
        Ok(operation) if args.len() == 2 => operation,
        Ok(_) => {
            return emit_error(&HelperError::InvalidArgument(
                "exactly one setup action is required".to_owned(),
            ));
        }
        Err(error) => return emit_error(&error),
    };

    match apply(operation) {
        Ok(changed) => {
            println!(
                "{{\"schema\":1,\"operation\":\"{}\",\"changed\":{}}}",
                operation.as_str(),
                changed
            );
            ExitCode::SUCCESS
        }
        Err(error) => emit_error(&error),
    }
}

fn emit_error(error: &HelperError) -> ExitCode {
    println!(
        "{{\"status\":\"error\",\"kind\":\"{}\",\"detail\":\"{}\"}}",
        error.kind(),
        json_escape(error.detail())
    );
    ExitCode::from(error.exit_code())
}

fn json_escape(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            character if character.is_control() => escaped.push(' '),
            character => escaped.push(character),
        }
    }
    escaped
}

fn apply(operation: Operation) -> Result<bool, HelperError> {
    #[cfg(unix)]
    {
        let path = Path::new(RULE_PATH);
        match operation {
            Operation::Install => install(path),
            Operation::Revert => revert(path),
        }
    }
    #[cfg(not(unix))]
    {
        let _ = operation;
        Err(HelperError::MissingDependency(
            "Linux udev setup is unavailable on this platform".to_owned(),
        ))
    }
}

#[cfg(unix)]
fn install(path: &Path) -> Result<bool, HelperError> {
    install_with(path, reload_udev)
}

#[cfg(unix)]
fn install_with<F>(path: &Path, reload: F) -> Result<bool, HelperError>
where
    F: FnOnce() -> Result<(), HelperError>,
{
    match fs::read(path) {
        Ok(existing) if existing == RULE_CONTENT.as_bytes() => Ok(false),
        Ok(_) => Err(HelperError::Conflict(format!(
            "refusing to overwrite a different file at {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            atomic_write(path)?;
            if let Err(error) = reload() {
                let _ = fs::remove_file(path);
                return Err(error);
            }
            Ok(true)
        }
        Err(error) => Err(HelperError::Io(format!(
            "cannot inspect {}: {error}",
            path.display()
        ))),
    }
}

#[cfg(unix)]
fn revert(path: &Path) -> Result<bool, HelperError> {
    revert_with(path, reload_udev)
}

#[cfg(unix)]
fn revert_with<F>(path: &Path, reload: F) -> Result<bool, HelperError>
where
    F: FnOnce() -> Result<(), HelperError>,
{
    match fs::read(path) {
        Ok(existing) if existing == RULE_CONTENT.as_bytes() => {
            fs::remove_file(path).map_err(|error| {
                HelperError::Io(format!("cannot remove {}: {error}", path.display()))
            })?;
            if let Err(error) = reload() {
                let _ = atomic_write(path);
                return Err(error);
            }
            Ok(true)
        }
        Ok(_) => Err(HelperError::Conflict(format!(
            "refusing to remove a different file at {}",
            path.display()
        ))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(HelperError::Io(format!(
            "cannot inspect {}: {error}",
            path.display()
        ))),
    }
}

#[cfg(unix)]
fn atomic_write(path: &Path) -> Result<(), HelperError> {
    let parent = path
        .parent()
        .ok_or_else(|| HelperError::Io(format!("setup path has no parent: {}", path.display())))?;
    fs::create_dir_all(parent)
        .map_err(|error| HelperError::Io(format!("cannot create {}: {error}", parent.display())))?;
    let temp_path = temporary_path(path);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp_path)
        .map_err(|error| {
            HelperError::Io(format!("cannot create {}: {error}", temp_path.display()))
        })?;
    let write_result = (|| -> io::Result<()> {
        file.write_all(RULE_CONTENT.as_bytes())?;
        file.sync_all()?;
        let mut permissions = file.metadata()?.permissions();
        use std::os::unix::fs::PermissionsExt;
        permissions.set_mode(0o644);
        fs::set_permissions(&temp_path, permissions)
    })();
    if let Err(error) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(HelperError::Io(format!(
            "cannot write {}: {error}",
            temp_path.display()
        )));
    }
    drop(file);
    // `rename` would atomically publish the file but would also replace a
    // target that appeared after the initial existence check. A same-directory
    // hard link publishes the inode atomically while refusing that overwrite.
    if let Err(error) = fs::hard_link(&temp_path, path) {
        let _ = fs::remove_file(&temp_path);
        return if error.kind() == io::ErrorKind::AlreadyExists {
            Err(HelperError::Conflict(format!(
                "refusing to overwrite a different file at {}",
                path.display()
            )))
        } else {
            Err(HelperError::Io(format!(
                "cannot install {}: {error}",
                path.display()
            )))
        };
    }
    if let Err(error) = fs::remove_file(&temp_path) {
        return Err(HelperError::Io(format!(
            "cannot clean up temporary setup file {}: {error}",
            temp_path.display()
        )));
    }
    Ok(())
}

#[cfg(unix)]
fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("taskmanager.rules");
    // A random suffix makes every attempt unique: a `kill -9` residue from a
    // previous run can never collide with (and therefore block) this run's
    // `create_new` publish step.
    path.with_file_name(format!(
        ".{name}.taskforest-{}-{}",
        std::process::id(),
        random_suffix()
    ))
}

/// A short random hex suffix from `/dev/urandom`, falling back to
/// pid-plus-clock nanos if the kernel entropy source is somehow unreadable.
/// Either way the value is unique per call; nothing here can panic.
#[cfg(unix)]
fn random_suffix() -> String {
    const BYTES: usize = 4;
    let mut entropy = [0u8; BYTES];
    let urandom = fs::File::open("/dev/urandom").and_then(|mut file| file.read_exact(&mut entropy));
    if urandom.is_ok() {
        entropy.iter().map(|byte| format!("{byte:02x}")).collect()
    } else {
        format!(
            "{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |elapsed| elapsed.as_nanos())
        )
    }
}

/// Per-stream capture cap for the bounded runner. `udevadm` diagnostics are a
/// few lines; 64 KiB is generous slack while bounding a runaway stream.
#[cfg(unix)]
const STREAM_CAP_BYTES: usize = 64 * 1024;

/// Deadline for one `udevadm` invocation. Rule reload/trigger are
/// non-interactive, so this only needs to cover a cold udev database — not a
/// human — and a stuck udevadm is killed rather than hanging first-run setup.
#[cfg(unix)]
const UDEVADM_DEADLINE: Duration = Duration::from_secs(60);

/// How long to wait for the drain thread after the child is gone; EOF is
/// normally observed immediately when the kernel closes the pipe.
#[cfg(unix)]
const DRAIN_GRACE: Duration = Duration::from_secs(2);

/// `try_wait` poll interval while waiting for the child to exit.
#[cfg(unix)]
const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// One bounded child result (see [`run_bounded`]).
#[cfg(unix)]
#[derive(Debug)]
struct BoundedChildOutput {
    status_code: Option<i32>,
    /// Retained for symmetry with the timeout path; the success path has no
    /// diagnostic consumer, so the drained bytes stay unread by design (the
    /// drain thread must be joined either way).
    #[allow(dead_code)]
    stderr: Vec<u8>,
}

/// Typed bounded-run failure. This standalone root binary must not depend on
/// `taskmanager-escalation` (dependency direction: the seam never becomes a
/// helper dependency), so it carries this equivalent inline implementation.
#[cfg(unix)]
#[derive(Debug)]
enum BoundedChildError {
    Spawn(io::Error),
    /// The deadline elapsed: the child was `SIGKILL`ed and reaped, with the
    /// partial stderr drained until then.
    TimedOut {
        stderr: Vec<u8>,
    },
    Wait(io::Error),
}

/// Spawn `command` and run it under a deadline with a bounded stderr drain:
/// an unbounded `output()` could hang this helper forever on a stuck udevadm
/// or balloon memory on a runaway diagnostic stream. Every path reaps the
/// child. Mirrors `taskmanager-escalation`'s `bounded_runner` (kept in sync
/// deliberately; the escalation copy is the canonical, fully tested one).
#[cfg(unix)]
fn run_bounded(
    command: &mut Command,
    timeout: Duration,
) -> Result<BoundedChildOutput, BoundedChildError> {
    let mut child = command.spawn().map_err(BoundedChildError::Spawn)?;
    let stderr_drain = child.stderr.take().map(spawn_drain);
    match wait_with_deadline(&mut child, timeout) {
        Ok(status_code) => Ok(BoundedChildOutput {
            status_code,
            stderr: finish_drain(stderr_drain),
        }),
        Err(error) if error.kind() == io::ErrorKind::TimedOut => {
            let _ = child.kill();
            let _ = child.wait();
            Err(BoundedChildError::TimedOut {
                stderr: finish_drain(stderr_drain),
            })
        }
        Err(error) => {
            let _ = child.kill();
            let _ = child.wait();
            Err(BoundedChildError::Wait(error))
        }
    }
}

/// Poll `try_wait` until the child exits or `timeout` elapses.
#[cfg(unix)]
fn wait_with_deadline(child: &mut Child, timeout: Duration) -> io::Result<Option<i32>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(status.code());
        }
        if Instant::now() >= deadline {
            return Err(io::Error::new(
                io::ErrorKind::TimedOut,
                "child did not exit within the bounded deadline",
            ));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// One background bounded drain of the captured stderr stream.
#[cfg(unix)]
struct Drain {
    buffer: Arc<Mutex<Vec<u8>>>,
    done: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

/// Read `source` until EOF, a read error, or the stream cap; reaching the cap
/// drops the pipe so a runaway writer fails instead of being buffered here.
#[cfg(unix)]
fn spawn_drain<R: Read + Send + 'static>(source: R) -> Drain {
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let thread_buffer = Arc::clone(&buffer);
    let thread_done = Arc::clone(&done);
    let handle = thread::spawn(move || {
        let mut source = source;
        let mut chunk = [0u8; 4096];
        loop {
            match source.read(&mut chunk) {
                Ok(0) => break,
                Ok(read) => {
                    let mut guard = match thread_buffer.lock() {
                        Ok(guard) => guard,
                        Err(poisoned) => poisoned.into_inner(),
                    };
                    let room = STREAM_CAP_BYTES.saturating_sub(guard.len());
                    guard.extend_from_slice(&chunk[..read.min(room)]);
                    if guard.len() >= STREAM_CAP_BYTES {
                        break;
                    }
                }
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
        thread_done.store(true, Ordering::Release);
    });
    Drain {
        buffer,
        done,
        handle: Some(handle),
    }
}

/// Collect the drain's bytes, waiting at most [`DRAIN_GRACE`] for its thread.
#[cfg(unix)]
fn finish_drain(drain: Option<Drain>) -> Vec<u8> {
    let Some(drain) = drain else {
        return Vec::new();
    };
    let deadline = Instant::now() + DRAIN_GRACE;
    while !drain.done.load(Ordering::Acquire) && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(1));
    }
    if drain.done.load(Ordering::Acquire)
        && let Some(handle) = drain.handle
    {
        let _ = handle.join();
    }
    match drain.buffer.lock() {
        Ok(guard) => guard.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

#[cfg(unix)]
fn reload_udev() -> Result<(), HelperError> {
    let candidates = ["/usr/bin/udevadm", "/bin/udevadm", "/sbin/udevadm"];
    for program in candidates {
        if !Path::new(program).is_file() {
            continue;
        }
        let reload = bounded_udevadm(program, ["control", "--reload-rules"])?;
        if reload != Some(0) {
            return Err(HelperError::Io(format!(
                "{program} control --reload-rules exited with {reload:?}"
            )));
        }
        let trigger = bounded_udevadm(program, ["trigger", "--subsystem-match=powercap"])?;
        if trigger != Some(0) {
            return Err(HelperError::Io(format!(
                "{program} trigger --subsystem-match=powercap exited with {trigger:?}"
            )));
        }
        return Ok(());
    }
    Err(HelperError::MissingDependency(
        "udevadm is not installed at a supported absolute path".to_owned(),
    ))
}

/// Run one fixed `udevadm` step under the bounded runner, mapping every
/// outcome onto the helper's typed error vocabulary.
#[cfg(unix)]
fn bounded_udevadm(
    program: &str,
    args: impl IntoIterator<Item = &'static str>,
) -> Result<Option<i32>, HelperError> {
    let mut command = Command::new(program);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    let output = run_bounded(&mut command, UDEVADM_DEADLINE).map_err(|error| match error {
        BoundedChildError::Spawn(error) | BoundedChildError::Wait(error) => {
            HelperError::Io(format!("cannot run {program}: {error}"))
        }
        BoundedChildError::TimedOut { stderr } => {
            // Keep whatever the stuck step managed to print (bounded), so a
            // hung udevadm still leaves a diagnostic trail.
            let mut detail = format!(
                "{program} did not finish within the {} s deadline and was killed",
                UDEVADM_DEADLINE.as_secs()
            );
            if !stderr.is_empty() {
                const PREFIX_BYTES: usize = 120;
                let bounded = String::from_utf8_lossy(&stderr[..stderr.len().min(PREFIX_BYTES)]);
                detail.push_str(&format!("; partial stderr: {bounded:?}"));
            }
            HelperError::Io(detail)
        }
    })?;
    Ok(output.status_code)
}

#[cfg(test)]
#[path = "../tests/headless/setup_helper_main.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
