//! Stateless CLI mode: dumps one point-in-time system snapshot as typed JSON.
//!
//! This is surviving innovation #6 — a scripting/piping surface that reuses the
//! toolkit-neutral app-host composition edge (the same provider registry the
//! GPUI shell uses) but never instantiates a GUI/TUI loop. One collection cycle, serialize,
//! print to stdout, exit 0.
//!
//! Honesty contract: an unavailable domain (no GPU, permission denied, an
//! `Unsupported` network) serializes as its typed `null`/discriminator — never
//! as a fabricated `0`. The snapshot is a single point in time; this module
//! does not stream and does not pretend a value it could not observe.

#![forbid(unsafe_code)]

mod suggest;

use std::error::Error;
use std::fmt;
use std::io::{self, Write};
use std::time::{Duration, Instant};

use taskmanager_application::{
    ContainerRollupEvent, HardwareInventoryEvent, NpuInventoryEvent, NpuInventoryRequest,
    PlatformClient, ProcessEvent, RefreshRequest,
};
use taskmanager_assets::product;
use taskmanager_core::core::hardware::HardwareInfo;
use taskmanager_core::core::npu::NpuInventorySnapshot;
use taskmanager_core::core::process_telemetry::ContainerSummary;
use taskmanager_core::{ProcessItem, SystemSnapshot};

#[cfg(feature = "ui-gpui")]
const FRONTEND_BINARY_NAME: &str = "taskforest-g";
#[cfg(feature = "ui-iced")]
const FRONTEND_BINARY_NAME: &str = "taskforest-i";
#[cfg(feature = "ui-tui")]
const FRONTEND_BINARY_NAME: &str = "taskmanager-tui";

/// Pause between non-blocking event drains while waiting for the runtime to
/// report a complete snapshot. The provider lanes run on their own OS threads;
/// the CLI thread only polls, so this stays off any UI loop.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Default wall-clock budget for one full six-domain collection cycle. On a
/// normal Linux host every domain reports within milliseconds; the bound only
/// guards a wedged provider.
const DEFAULT_COLLECTION_TIMEOUT: Duration = Duration::from_secs(5);

/// CLI dispatch result produced by [`parse_args`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliMode {
    /// Launch the compiled-in UI frontend (ADR-029): the GPUI desktop
    /// frontend by default, or the TUI/iced frontend when built with the
    /// corresponding `ui-*` feature. `demo` runs the fixture data with no
    /// host I/O (supported by the TUI/iced shapes; the GPUI shape reports
    /// that demo mode is not yet supported).
    Gui { app_id: Option<String>, demo: bool },
    /// Headless terminal text-frame evidence (`ui-tui` shape only): render a
    /// fixed-size frame of the demo app to stdout and exit. Width/height
    /// default to 120×36; other shapes report that the mode is not supported.
    Snapshot { width: u16, height: u16 },
    /// Dump a one-shot JSON snapshot to stdout and exit 0.
    JsonSnapshot,
    /// Print per-metric threshold suggestions as JSON to stdout and exit 0.
    /// One snapshot yields at most one sample per metric, so every numeric
    /// metric is honestly `Insufficient` (`too_few_samples`) until a real
    /// rolling window feeds the heuristic — never a fabricated threshold.
    SuggestThresholds,
    /// Drive the per-feature Intel-PMU privilege escalation (ADR-023): invoke
    /// the polkit/pkexec helper ON DEMAND from this unprivileged process and
    /// print the typed per-engine GPU utilization (or a typed honest denial) as
    /// JSON. The main app is NOT elevated; the prompt fires only because this
    /// flag was passed.
    GpuEngines,
    /// Windows+GPUI evidence mode: open the real app window, capture its own
    /// composited frames once (Windows.Graphics.Capture through zed-scap), and
    /// write `capture.png` + metadata + a manifest line into `out`, then exit.
    /// Other platforms/shapes report that the mode is not supported.
    CaptureWindow { out: std::path::PathBuf },
    /// Print the help text and exit 0.
    Help,
}

/// Stable failure reason for argv parsing. The detail text is stable for logs.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CliArgError {
    /// An unrecognized flag or an unexpected trailing argument was supplied.
    UnknownArgument,
    /// `--app-id`/`-a` was supplied without a value.
    MissingApplicationId,
    /// `--capture-window` was supplied without an output directory.
    MissingCaptureOutput,
    /// The requested application ID cannot be used as a desktop application ID.
    InvalidApplicationId,
    /// `--snapshot` received a non-numeric dimension.
    InvalidDimension(String),
}

impl CliArgError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownArgument => "unknown_argument",
            Self::MissingApplicationId => "missing_application_id",
            Self::MissingCaptureOutput => "missing_capture_output",
            Self::InvalidApplicationId => "invalid_application_id",
            Self::InvalidDimension(_) => "invalid_snapshot_dimension",
        }
    }
}

impl fmt::Display for CliArgError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownArgument => formatter.write_str(
                "unknown argument; use --app-id/-a, --demo, --json, --suggest-thresholds, --gpu-engines, --help (or no flag to launch the GUI)",
            ),
            Self::MissingApplicationId => {
                formatter.write_str("--app-id/-a requires an application ID")
            }
            Self::MissingCaptureOutput => {
                formatter.write_str("--capture-window requires an output directory")
            }
            Self::InvalidDimension(dimension) => {
                formatter.write_str("invalid snapshot dimension: ")?;
                formatter.write_str(dimension)
            }
            Self::InvalidApplicationId => formatter.write_str(
                "invalid application ID; use ASCII letters, digits, hyphens, and periods with at least one period",
            ),
        }
    }
}

impl Error for CliArgError {}

/// Parse argv (excluding the binary name, `argv[0]`).
///
/// A single leading flag selects the mode (ADR-029 unified CLI): `--app-id`/`-a`
/// configures the GUI's desktop identity; `--demo` runs the compiled-in UI on
/// fixture data; `--snapshot [W H]` renders headless TUI text-frame evidence;
/// `--json`/`-j` request a JSON snapshot;
/// `--suggest-thresholds` requests per-metric threshold suggestions as JSON;
/// `--help`/`-h` requests help; no flag launches the compiled-in UI. Any other
/// token, or a trailing argument after a mode flag, is rejected so a scripting
/// typo cannot silently produce output the caller did not ask for.
pub fn parse_args<I>(args: I) -> Result<CliMode, CliArgError>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    match args.next().as_deref() {
        None => Ok(CliMode::Gui {
            app_id: None,
            demo: false,
        }),
        Some("--app-id") | Some("-a") => {
            let value = args.next().ok_or(CliArgError::MissingApplicationId)?;
            if args.next().is_some() {
                return Err(CliArgError::UnknownArgument);
            }
            parse_gui_application_id(value)
        }
        Some(value) if value.starts_with("--app-id=") => {
            let value = value.trim_start_matches("--app-id=").to_owned();
            if args.next().is_some() {
                return Err(CliArgError::UnknownArgument);
            }
            parse_gui_application_id(value)
        }
        Some("--snapshot") => {
            let width = parse_dimension(args.next(), 120, "width")?;
            let height = parse_dimension(args.next(), 36, "height")?;
            if args.next().is_some() {
                return Err(CliArgError::UnknownArgument);
            }
            Ok(CliMode::Snapshot { width, height })
        }
        Some("--demo") => match args.next() {
            None => Ok(CliMode::Gui {
                app_id: None,
                demo: true,
            }),
            Some(_) => Err(CliArgError::UnknownArgument),
        },
        Some("--json") | Some("-j") => match args.next() {
            None => Ok(CliMode::JsonSnapshot),
            Some(_) => Err(CliArgError::UnknownArgument),
        },
        Some("--suggest-thresholds") => match args.next() {
            None => Ok(CliMode::SuggestThresholds),
            Some(_) => Err(CliArgError::UnknownArgument),
        },
        Some("--gpu-engines") => match args.next() {
            None => Ok(CliMode::GpuEngines),
            Some(_) => Err(CliArgError::UnknownArgument),
        },
        Some("--capture-window") => {
            let value = args.next().ok_or(CliArgError::MissingCaptureOutput)?;
            if value.is_empty() {
                return Err(CliArgError::MissingCaptureOutput);
            }
            if args.next().is_some() {
                return Err(CliArgError::UnknownArgument);
            }
            Ok(CliMode::CaptureWindow {
                out: std::path::PathBuf::from(value),
            })
        }
        Some("--help") | Some("-h") => match args.next() {
            None => Ok(CliMode::Help),
            Some(_) => Err(CliArgError::UnknownArgument),
        },
        Some(_) => Err(CliArgError::UnknownArgument),
    }
}

fn parse_dimension(value: Option<String>, default: u16, name: &str) -> Result<u16, CliArgError> {
    match value {
        None => Ok(default),
        Some(value) => value
            .parse::<u16>()
            .map_err(|_| CliArgError::InvalidDimension(name.to_owned())),
    }
}

fn parse_gui_application_id(value: String) -> Result<CliMode, CliArgError> {
    if !is_valid_application_id(&value) {
        return Err(CliArgError::InvalidApplicationId);
    }
    Ok(CliMode::Gui {
        app_id: Some(value),
        demo: false,
    })
}

/// Validate the reverse-DNS style ID accepted by desktop application systems.
/// Keep this toolkit-neutral: the composition edge passes the already-validated
/// string into GPUI's `WindowOptions` and no provider or persistence layer sees it.
fn is_valid_application_id(value: &str) -> bool {
    !value.is_empty()
        && value.contains('.')
        && !value.starts_with('.')
        && !value.ends_with('.')
        && !value.contains("..")
        && value.split('.').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        })
}

/// Typed failure stage for a one-shot snapshot collection.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SnapshotCliErrorKind {
    /// The event port reported an unrecoverable receive error.
    Drain,
    /// No complete snapshot arrived before the collection budget elapsed. The
    /// CLI never fabricates a partial snapshot into a `0`-filled document — it
    /// reports this typed timeout instead.
    CollectionTimeout,
}

impl SnapshotCliErrorKind {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Drain => "drain",
            Self::CollectionTimeout => "collection_timeout",
        }
    }
}

/// Typed one-shot snapshot failure carrying a host-specific detail message.
#[derive(Debug)]
pub struct SnapshotCliError {
    kind: SnapshotCliErrorKind,
    detail: String,
}

impl SnapshotCliError {
    fn new(kind: SnapshotCliErrorKind, detail: impl Into<String>) -> Self {
        Self {
            kind,
            detail: detail.into(),
        }
    }

    #[must_use]
    pub const fn kind(&self) -> SnapshotCliErrorKind {
        self.kind
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }
}

impl fmt::Display for SnapshotCliError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.kind.code(), self.detail)
    }
}

impl Error for SnapshotCliError {}

/// One complete collection cycle: the six-domain snapshot, the process list,
/// and any container rollup that arrived in the same wave. Containers are
/// optional (a cgroup-v1 host, or a host with no containers running, never
/// produces a `ContainerRollupEvent`) and stay an honest empty when absent.
struct Collected {
    snapshot: SystemSnapshot,
    processes: Vec<ProcessItem>,
    containers: Vec<ContainerSummary>,
    /// Static hardware facts; `None` when no inventory snapshot arrived in
    /// the collection window (the envelope then omits the key — honest).
    hardware: Option<HardwareInfo>,
    /// NPU accelerator inventory; `None` when no answer arrived. An empty
    /// device list (Some) is the honest no-NPU host, never an error.
    npu_inventory: Option<NpuInventorySnapshot>,
}

/// One-shot snapshots wait this long before their first (and only) refresh:
/// rate facts — CPU usage, disk and network throughput — are deltas between
/// two samples at least one minimum CPU update interval apart, and the
/// platform providers take their baseline when the runtime spawns. The wait
/// lets the single refresh observe a real delta instead of an honestly
/// unavailable rate family.
const RATE_SAMPLING_WARMUP: Duration = Duration::from_millis(250);

/// Drive one bounded collection cycle against an already-spawned client and
/// return the snapshot, process list, and any container rollup.
///
/// Requests three independent refresh axes — six-domain telemetry, the process
/// list, and the container rollup — and drains until both a complete snapshot
/// and a process list have arrived (or `timeout` elapses). Containers are
/// opportunistically folded from any event that arrives in the same wave; they
/// never gate readiness, so a host without cgroup-v2 (or with no containers)
/// returns an honest empty list instead of blocking until the timeout.
fn collect_one_snapshot(
    client: &mut PlatformClient,
    timeout: Duration,
) -> Result<Collected, SnapshotCliError> {
    std::thread::sleep(RATE_SAMPLING_WARMUP);
    let submitted_at_ms = wall_clock_ms();
    // Three independent refresh axes publishing on separate lanes. Either of
    // the first two may complete first; containers are best-effort.
    let _ = client.request_refresh(RefreshRequest::Telemetry, submitted_at_ms);
    let _ = client.request_refresh(RefreshRequest::Processes, submitted_at_ms);
    let _ = client.request_refresh(RefreshRequest::Containers, submitted_at_ms);
    // Static-facts side channels for the receipt envelope: one hardware
    // inventory refresh plus one bounded accelerator read. Both ride the same
    // collection window; absence stays an honest `None`.
    let _ = client.request_refresh(RefreshRequest::HardwareInventory, submitted_at_ms);
    let _ = client.submit_npu_inventory(NpuInventoryRequest {}, submitted_at_ms);

    let started = Instant::now();
    let mut snapshot: Option<SystemSnapshot> = None;
    let mut processes: Vec<ProcessItem> = Vec::new();
    let mut have_processes = false;
    let mut containers: Vec<ContainerSummary> = Vec::new();
    let mut hardware: Option<HardwareInfo> = None;
    let mut npu_inventory: Option<NpuInventorySnapshot> = None;

    loop {
        let batch = client.try_drain().map_err(|error| {
            SnapshotCliError::new(SnapshotCliErrorKind::Drain, format!("{error:?}"))
        })?;
        // The latest terminal projection that can assemble a render snapshot
        // wins; partial projections never fabricate missing domains. The CLI
        // follows the frontend's projection rule (render_snapshot): the five
        // core domains must be current, while a legitimately typed-unavailable
        // GPU domain (e.g. a Windows host without the NVML source) stays an
        // honest gap serialized as null instead of blocking the whole
        // snapshot until the collection timeout.
        for projection in batch.system_telemetry_projections {
            if let Some(complete) = projection
                .complete_snapshot()
                .or_else(|| projection.render_snapshot())
            {
                snapshot = Some(complete);
            }
        }
        // The process list is a full replace per `ProcessEvent::Snapshot`; the
        // most recently published list is the point-in-time truth.
        for correlated in batch.process_events {
            if let ProcessEvent::Snapshot(items) = correlated.event {
                processes = items;
                have_processes = true;
            }
        }
        // The container rollup is a full replace per `ContainerRollupEvent`;
        // the last event's typed summaries win. A typed-unavailable rollup
        // carries an empty container list, which we preserve honestly. The
        // enum has a single `Snapshot` variant today; the plain destructure
        // breaks loudly if a second variant is added later.
        for correlated in batch.containers_events {
            let ContainerRollupEvent::Snapshot(rollup) = correlated.event;
            containers = rollup.containers;
        }
        // Hardware facts and the accelerator inventory are latest-wins side
        // channels; a typed NPU failure snapshot is preserved verbatim.
        for correlated in batch.hardware_inventory_events {
            let HardwareInventoryEvent::Snapshot(snapshot) = correlated.event;
            hardware = Some(snapshot.value);
        }
        for correlated in batch.npu_inventory_events {
            let NpuInventoryEvent::Update(snapshot) = correlated.event;
            npu_inventory = Some(snapshot);
        }

        // Hardware inventory rides a required lane, so waiting for it keeps
        // the envelope's static-facts block deterministic instead of racing
        // the first collection wave on a cold host. The NPU read stays
        // outside the break condition: a runtime without the optional facet
        // rejects the submission and the key is honestly absent.
        if snapshot.is_some() && have_processes && hardware.is_some() {
            break;
        }
        if started.elapsed() >= timeout {
            break;
        }
        std::thread::sleep(POLL_INTERVAL);
    }

    let snapshot = snapshot.ok_or_else(|| {
        SnapshotCliError::new(
            SnapshotCliErrorKind::CollectionTimeout,
            format!(
                "no complete six-domain snapshot arrived within {} ms",
                timeout.as_millis()
            ),
        )
    })?;

    // Best-effort final drain: a container rollup often arrives in the same
    // collection wave as the snapshot but one poll later. One non-blocking
    // drain catches it without blocking when the capability is absent (the
    // batch is then empty and the loop body is a no-op). We never fabricate
    // containers — an absent event leaves the honest empty list in place.
    if let Ok(batch) = client.try_drain() {
        for correlated in batch.containers_events {
            let ContainerRollupEvent::Snapshot(rollup) = correlated.event;
            containers = rollup.containers;
        }
        for correlated in batch.hardware_inventory_events {
            let HardwareInventoryEvent::Snapshot(snapshot) = correlated.event;
            hardware = Some(snapshot.value);
        }
        for correlated in batch.npu_inventory_events {
            let NpuInventoryEvent::Update(snapshot) = correlated.event;
            npu_inventory = Some(snapshot);
        }
    }

    Ok(Collected {
        snapshot,
        processes,
        containers,
        hardware,
        npu_inventory,
    })
}

/// Collect one point-in-time snapshot from an already-composed platform client
/// and return it as a pretty-printed JSON string.
///
/// The toolkit-neutral app host owns native runtime and client construction;
/// this module only drives the application port. Once given the client, this
/// collector requests one telemetry, one process, and one container refresh,
/// drains the resulting events until both a complete six-domain snapshot and
/// a process list have arrived (or `timeout` elapses), then serializes via the
/// shared `snapshot_to_json_with_extras` formatter. The runtime lanes keep
/// running until the process exits; this function does not stream.
pub fn collect_json_snapshot_from_client(
    mut client: PlatformClient,
    timeout: Duration,
) -> Result<String, SnapshotCliError> {
    let collected = collect_one_snapshot(&mut client, timeout)?;
    // Per-process GPU engines are scanned in one bounded, non-blocking bulk pass
    // (Linux /proc fdinfo; an honest empty array on hosts without that source)
    // and folded into the shared snapshot envelope. Threshold suggestions live
    // on the dedicated `--suggest-thresholds` path, not this snapshot envelope.
    Ok(crate::cli_process_gpu::render_json_snapshot(
        &collected.snapshot,
        &collected.processes,
        &collected.containers,
        collected.hardware.as_ref(),
        collected.npu_inventory.as_ref(),
        wall_clock_ms(),
    ))
}

/// Run the JSON snapshot mode against stdout for an already-composed platform
/// client: collect using the default budget, print, return Ok on success. A
/// typed collection failure is returned as an `io::Error` so the binary can
/// surface it on stderr and exit non-zero without panicking.
pub fn run_json_snapshot_with(client: PlatformClient) -> io::Result<()> {
    let json = collect_json_snapshot_from_client(client, DEFAULT_COLLECTION_TIMEOUT)
        .map_err(|error| io::Error::other(format!("taskmanager --json: {error}")))?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(json.as_bytes())?;
    // Trailing newline so downstream tools (jq, grep, wc) see a clean record.
    handle.write_all(b"\n")?;
    Ok(())
}

/// Collect one point-in-time snapshot from an already-composed platform client
/// and return per-metric threshold suggestions as a pretty-printed JSON string.
///
/// Reuses `collect_one_snapshot` so the collection path is identical to the
/// `--json` mode; only the rendering differs. The suggestion document is keyed
/// by metric (see `metric_key`); see `suggest_thresholds_json` for the
/// honesty contract on insufficient data.
pub fn collect_suggest_thresholds_from_client(
    mut client: PlatformClient,
    timeout: Duration,
) -> Result<String, SnapshotCliError> {
    let collected = collect_one_snapshot(&mut client, timeout)?;
    Ok(suggest::suggest_thresholds_json(&collected.snapshot))
}

/// Run the `--suggest-thresholds` mode against stdout for an already-composed
/// platform client: collect one snapshot using the default budget, render the
/// per-metric threshold suggestions as JSON, print, return Ok on success.
pub fn run_suggest_thresholds_with(client: PlatformClient) -> io::Result<()> {
    let json = collect_suggest_thresholds_from_client(client, DEFAULT_COLLECTION_TIMEOUT)
        .map_err(|error| io::Error::other(format!("taskmanager --suggest-thresholds: {error}")))?;
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    handle.write_all(json.as_bytes())?;
    handle.write_all(b"\n")?;
    Ok(())
}

/// Write the help text to `writer`. Used for both `--help` (stdout) and the
/// usage-on-error path (stderr).
pub fn print_help_to(writer: &mut impl Write) -> io::Result<()> {
    let binary = FRONTEND_BINARY_NAME;
    writeln!(writer, "{} — {}", product::NAME, product::TAGLINE_EN)?;
    writeln!(writer)?;
    writeln!(
        writer,
        "  {binary}                 launch the compiled-in system monitor"
    )?;
    writeln!(
        writer,
        "  {binary} --app-id ID, -a ID  set a custom desktop application ID"
    )?;
    writeln!(
        writer,
        "  {binary} --json, -j      dump a one-shot system snapshot as typed JSON to stdout"
    )?;
    writeln!(writer, "  {binary} --suggest-thresholds")?;
    writeln!(
        writer,
        "                             print per-metric alert threshold suggestions as JSON"
    )?;
    writeln!(
        writer,
        "  {binary} --gpu-engines   read Intel GPU engine utilization via the polkit"
    )?;
    writeln!(
        writer,
        "  {binary} --demo            run the compiled-in UI with fixture data (no host I/O)"
    )?;
    writeln!(
        writer,
        "  {binary} --snapshot [W H]  headless text-frame evidence (ui-tui shape; default 120x36)"
    )?;
    writeln!(
        writer,
        "                             per-feature helper (prompts once via pkexec) and print"
    )?;
    writeln!(
        writer,
        "                             typed JSON; the main app stays unprivileged"
    )?;
    writeln!(writer, "  {binary} --capture-window DIR")?;
    writeln!(
        writer,
        "                             capture this app window once to DIR (Windows+GPUI"
    )?;
    writeln!(
        writer,
        "                             evidence mode: writes capture.png + metadata, then exits)"
    )?;
    writeln!(writer, "  {binary} --help, -h      show this help")?;
    writeln!(writer)?;
    writeln!(
        writer,
        "The --json mode is stateless: it collects one point-in-time snapshot"
    )?;
    writeln!(
        writer,
        "(process list, CPU, memory, disks, network interfaces, GPUs, container"
    )?;
    writeln!(writer, "rollup when cgroup-v2 is available) and exits 0.")?;
    writeln!(
        writer,
        "Unavailable telemetry (no GPU, permission denied, unsupported fields)"
    )?;
    writeln!(
        writer,
        "serializes as null or a typed discriminator, never as a fabricated 0."
    )?;
    writeln!(
        writer,
        "The --suggest-thresholds mode derives one sample per metric from a"
    )?;
    writeln!(
        writer,
        "single snapshot, so every numeric metric is honestly reported as"
    )?;
    writeln!(
        writer,
        "insufficient (too_few_samples) until a real rolling window feeds the"
    )?;
    writeln!(
        writer,
        "heuristic — it never prints a fabricated threshold."
    )?;
    Ok(())
}

fn wall_clock_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| u64::try_from(duration.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

#[cfg(test)]
#[path = "../tests/logic/cli_tests.rs"]
mod tests;
