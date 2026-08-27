//! Pure-safe per-process network attribution for the AF_PACKET capture loop.
//!
//! Joins a captured packet's [`FiveTuple`] (source/dest IP + protocol + ports,
//! produced by the audited `taskmanager-afpacket` seam's safe parser) to the pid
//! that owns the matching socket. This is the safe consumer of the boundary
//! crate — there is NO `unsafe` here. Attribution is a two-step join:
//!   1. *endpoint pair → socket inode*: read the host socket tables
//!      (`/proc/net/{tcp,tcp6,udp,udp6}`), index each established flow by its
//!      protocol + the canonical (sorted) pair of endpoints — direction-agnostic,
//!      because a connection is identified by the unordered endpoint pair;
//!   2. *inode → pid*: read every `/proc/<pid>/fd` readlink, recording which pid
//!      holds each `socket:[<inode>]`.
//!
//! A packet then resolves: canonicalize its `(src,dst)` the same way → look up
//! the inode → look up the owner pid. Receive-vs-transmit direction is NOT this
//! module's concern (the capture loop decides via `sll_pkttype` or a local-addr
//! heuristic); attribution only answers "which pid does this flow belong to?".
//! Limitations (MVP): sockets shared across pids (fork, fd-passing) attribute to
//! one representative owner; sockets in non-init network namespaces (containers)
//! are not matched against the host tables.
//!
//! ## The escalation pattern for fd-returning helpers (ADR-024/025)
//!
//! THE pattern for consuming an fd-returning escalation is the **capability
//! lane + runtime UI swap**, not a constructor flag: the backend starts
//! prompt-free ([`AfPacketAccountingBackend::start`] — direct open, typed
//! degrade on denial), the runtime exposes a one-shot escalation capability
//! (`process.network.escalation`, offered from the UI as an "enable
//! per-process network" pill), and granting it invokes the launcher
//! (`invoke_net_launcher`, object-safe seam) and swaps the shared accounting
//! handle to [`AfPacketAccountingBackend::start_from_source`] over the received
//! fd — see `NativeProcessNetworkEscalationProvider` in
//! `provider/process.rs`. Construction NEVER consults the launcher; a
//! `start_with_launcher(Some(..))` constructor-escalation variant was removed
//! as production-dead (test-only) — the swap-on-grant flow above is the only
//! operational consumer wiring.

use std::collections::{HashMap, HashSet};
use std::io;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use taskmanager_afpacket::{CapturedPacket, FiveTuple, PacketSource, five_tuple};
use taskmanager_core::ProcessIdentity;

use super::network::{
    NetworkAccountingFailure, NetworkByteCounters, ProcessNetworkAccountingBackend,
    parse_socket_table,
};
use super::{ConnectionAddressFamily, ConnectionEndpoint, ConnectionTransport};

const TCP: u8 = 6;
const UDP: u8 = 17;

/// Protocol + canonical (low, high) endpoint pair → owning socket inode. Sorting
/// the two endpoints makes the lookup match a packet regardless of direction.
type FlowKey = (u8, SocketAddr, SocketAddr);

/// The `/proc`-derived attribution index: which pid owns each socket inode, and
/// which (protocol, endpoint-pair) maps to that inode. `live_pids` records the
/// pid population the index was built from (every numeric `/proc/<pid>` dir,
/// regardless of fd readability) — the authoritative set per-pid counters are
/// pruned against at each rebuild.
#[derive(Debug, Default)]
pub struct PacketAttribution {
    owners: HashMap<u64, u32>,
    flows: HashMap<FlowKey, u64>,
    live_pids: HashSet<u32>,
}

impl PacketAttribution {
    /// Build the index from a `/proc` root (host view). Reads `/proc/<pid>/fd`
    /// for every pid (inode→pid owners) and `/proc/net/{tcp,tcp6,udp,udp4}`
    /// (endpoint-pair→inode). Best-effort: unreadable entries are skipped.
    #[must_use]
    pub fn from_proc_root(proc_root: &Path) -> Self {
        let mut owners = HashMap::new();
        let mut live_pids = HashSet::new();
        if let Ok(pids) = std::fs::read_dir(proc_root) {
            for pid_entry in pids.flatten() {
                let Some(pid) = entry_pid(&pid_entry) else {
                    continue;
                };
                live_pids.insert(pid);
                let fd_dir = pid_entry.path().join("fd");
                if let Ok(fds) = std::fs::read_dir(fd_dir) {
                    for fd in fds.flatten() {
                        if let Ok(target) = std::fs::read_link(fd.path())
                            && let Some(inode) = socket_inode(&target.to_string_lossy())
                        {
                            owners.insert(inode, pid);
                        }
                    }
                }
            }
        }
        let mut flows = HashMap::new();
        for (file, transport, family) in INET_SOCKET_TABLES {
            let Some(proto) = transport_proto(&transport) else {
                continue;
            };
            if let Ok(text) = std::fs::read_to_string(proc_root.join("net").join(file)) {
                for connection in parse_socket_table(&text, transport, family) {
                    let (Some(local), Some(remote)) = (
                        endpoint_addr(&connection.local),
                        endpoint_addr(&connection.remote),
                    ) else {
                        continue;
                    };
                    let Some(inode) = connection
                        .provider_key
                        .as_ref()
                        .and_then(|key| key.as_numeric())
                    else {
                        continue;
                    };
                    let (lo, hi) = sort_pair(local, remote);
                    flows.insert((proto, lo, hi), inode);
                }
            }
        }
        Self {
            owners,
            flows,
            live_pids,
        }
    }

    /// Resolve a captured packet's [`FiveTuple`] to the owning pid, or `None`
    /// when no matching established flow is known or the flow's inode has no
    /// recorded owner (the pid may have exited between index build and capture).
    #[must_use]
    pub fn attribute(&self, tuple: &FiveTuple) -> Option<u32> {
        if tuple.proto != TCP && tuple.proto != UDP {
            return None;
        }
        let src = SocketAddr::new(tuple.src, tuple.src_port);
        let dst = SocketAddr::new(tuple.dst, tuple.dst_port);
        let (lo, hi) = sort_pair(src, dst);
        let inode = self.flows.get(&(tuple.proto, lo, hi))?;
        self.owners.get(inode).copied()
    }
}

/// Per-pid byte counters shared between the capture worker thread and the
/// backend's synchronous [`AfPacketAccountingBackend::read_counters`].
type CounterMap = Arc<Mutex<HashMap<u32, NetworkByteCounters>>>;

/// Capture buffer: one jumbo frame (max Ethernet MTU + VLAN) is plenty.
const CAPTURE_BUFFER_BYTES: usize = 65_536;
/// How often the worker rebuilds the `/proc` attribution index. Socket→pid
/// ownership and the socket tables churn as processes connect/exit; a couple of
/// seconds keeps attribution fresh without re-walking `/proc` per packet.
const ATTRIBUTION_REFRESH: Duration = Duration::from_secs(2);

/// Per-process network byte accounting backed by the audited `AF_PACKET` seam.
///
/// On construction it probes for `CAP_NET_RAW`: without it (the default
/// unprivileged build) the socket open fails fast with `EPERM`, the backend
/// records [`NetworkAccountingFailure::PermissionDenied`] and runs NO capture
/// thread — every `read_counters` then yields that failure (honest: per-process
/// net needs the capability). With the capability (a pkexec'd launcher —
/// `taskmanager-net-launcher`, per ADR-024/025 + the polkit policy — passing
/// the fd, or a `CAP_NET_RAW` host) a worker thread attributes each frame's
/// 5-tuple to a pid via [`PacketAttribution`] and accumulates per-pid rx/tx
/// counters; this live path is not exercised unprivileged and is not
/// runtime-verified headless, but the ingest logic is unit-tested.
///
/// Direction (rx vs tx) comes from the captured frame's `sll_pkttype`;
/// counters are keyed by pid, so a pid reuse between attribution and
/// `read_counters` is the tracker's identity check to catch (an accepted MVP
/// limitation, as is single-representative ownership for fd-shared sockets).
#[derive(Debug)]
pub struct AfPacketAccountingBackend {
    state: BackendState,
}

#[derive(Debug)]
enum BackendState {
    /// Open failed (typically `EPERM` without `CAP_NET_RAW`); no worker thread.
    Degraded(NetworkAccountingFailure),
    /// Worker running; counters shared with `read_counters`.
    Capturing {
        counters: CounterMap,
        shutdown: Arc<AtomicBool>,
        _worker: JoinHandle<()>,
    },
}

impl AfPacketAccountingBackend {
    /// Probe for `CAP_NET_RAW` and, on success, start the capture worker bound
    /// to `iface_index`. `proc_root` feeds attribution; `iface_index` selects
    /// the single NIC whose traffic is captured (cost control). Always returns a
    /// backend — failure is a *state* (degraded), so the collector can degrade.
    /// Non-intrusive default: no launcher seam is consulted (there is none to
    /// consult here — see the module doc's capability-lane pattern), so an
    /// unprivileged host degrades to `RequiresEscalation` without ever
    /// prompting.
    #[must_use]
    pub fn start(proc_root: &Path, iface_index: u32) -> Self {
        let source = match PacketSource::open(iface_index) {
            Ok(source) => source,
            Err(error) => {
                return Self {
                    state: BackendState::Degraded(classify_afpacket_io(&error)),
                };
            }
        };
        Self::start_from_source(source, proc_root)
    }

    /// Start the worker over an already-obtained packet source — either a
    /// direct unprivileged open or an escalated fd (ADR-024/025); both reach
    /// the same capture loop. The escalation provider swaps this in over the
    /// shared accounting handle so the next observation reports real rates.
    pub(crate) fn start_from_source(source: PacketSource, proc_root: &Path) -> Self {
        let counters: CounterMap = Arc::new(Mutex::new(HashMap::new()));
        let shutdown = Arc::new(AtomicBool::new(false));
        let worker = CaptureWorker {
            source,
            counters: counters.clone(),
            shutdown: shutdown.clone(),
            attribution: PacketAttribution::from_proc_root(proc_root),
            proc_root: proc_root.to_path_buf(),
            last_refresh: Instant::now(),
        };
        let join = match thread::Builder::new()
            .name("tm-afpacket-capture".into())
            .spawn(move || worker.run())
        {
            Ok(handle) => handle,
            // Could not spawn the worker (e.g. resource limits) — degrade
            // honestly rather than panic at collector construction.
            Err(_) => {
                return Self {
                    state: BackendState::Degraded(NetworkAccountingFailure::Unavailable),
                };
            }
        };
        Self {
            state: BackendState::Capturing {
                counters,
                shutdown,
                _worker: join,
            },
        }
    }
}

impl ProcessNetworkAccountingBackend for AfPacketAccountingBackend {
    fn read_counters(
        &mut self,
        identity: ProcessIdentity,
        _now_ms: u64,
    ) -> Result<NetworkByteCounters, NetworkAccountingFailure> {
        match &self.state {
            BackendState::Degraded(failure) => Err(*failure),
            BackendState::Capturing { counters, .. } => {
                // Recover from a poisoned lock (a worker panic) rather than
                // panicking at collection time — surface whatever counts exist.
                let map = counters
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                Ok(map
                    .get(&identity.pid)
                    .copied()
                    .unwrap_or(NetworkByteCounters {
                        rx_bytes: 0,
                        tx_bytes: 0,
                    }))
            }
        }
    }
}

impl Drop for AfPacketAccountingBackend {
    fn drop(&mut self) {
        if let BackendState::Capturing { shutdown, .. } = &self.state {
            // The worker wakes within one recv timeout (200 ms) and exits. The
            // JoinHandle is dropped without joining (daemon-style) so teardown
            // never blocks the collector.
            shutdown.store(true, Ordering::Release);
        }
    }
}

struct CaptureWorker {
    source: PacketSource,
    counters: CounterMap,
    shutdown: Arc<AtomicBool>,
    attribution: PacketAttribution,
    proc_root: PathBuf,
    last_refresh: Instant,
}

impl CaptureWorker {
    fn run(mut self) {
        let mut buf = vec![0u8; CAPTURE_BUFFER_BYTES];
        while !self.shutdown.load(Ordering::Acquire) {
            if self.last_refresh.elapsed() >= ATTRIBUTION_REFRESH {
                self.attribution = PacketAttribution::from_proc_root(&self.proc_root);
                self.last_refresh = Instant::now();
                // Prune counters together with the rebuild that proves which
                // pids still exist.
                retain_live_counters(&self.counters, &self.attribution.live_pids);
            }
            match self.source.recv(&mut buf) {
                Ok(packet) => self.ingest(packet),
                // Recv timeout (SO_RCVTIMEO) — loop and re-check the shutdown flag.
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => continue,
                // Socket closed / interface gone — stop the worker cleanly.
                Err(_) => break,
            }
        }
    }

    fn ingest(&self, packet: CapturedPacket) {
        accumulate(&self.attribution, &self.counters, packet);
    }
}

/// Drop per-pid counters whose pid is absent from the fresh `/proc`
/// enumeration. Between a process's exit and the next rebuild, the stale
/// index may still attribute its lingering sockets' packets (up to one
/// `ATTRIBUTION_REFRESH` window) — that short tail is exactly why pruning
/// waits for the rebuild instead of running per packet. At the rebuild the
/// boundary is clean and immediate: the fresh owner walk no longer resolves
/// the exited pid's inodes, so its entry can neither grow again nor be read
/// (identity validation rejects an exited target before `read_counters`) —
/// it can only leak, or pollute the baseline if the pid is reused.
/// Fail-closed: an empty live set (the `/proc` walk failed) prunes nothing
/// rather than wiping counters for processes that are still alive.
fn retain_live_counters(counters: &CounterMap, live_pids: &HashSet<u32>) {
    if live_pids.is_empty() {
        return;
    }
    counters
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .retain(|pid, _| live_pids.contains(pid));
}

/// Attribute one captured frame to a pid and accumulate its byte length into the
/// shared rx/tx counters. Extracted from the worker so the attribution +
/// accumulation logic is unit-testable without `CAP_NET_RAW`.
fn accumulate(attribution: &PacketAttribution, counters: &CounterMap, packet: CapturedPacket) {
    let Some(tuple) = five_tuple(packet.frame) else {
        return;
    };
    let Some(pid) = attribution.attribute(&tuple) else {
        return;
    };
    let len = u64::try_from(packet.frame.len()).unwrap_or(u64::MAX);
    let mut map = counters
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let entry = map.entry(pid).or_insert(NetworkByteCounters {
        rx_bytes: 0,
        tx_bytes: 0,
    });
    if packet.outgoing {
        entry.tx_bytes = entry.tx_bytes.saturating_add(len);
    } else {
        entry.rx_bytes = entry.rx_bytes.saturating_add(len);
    }
}

/// Resolve the host's default-route interface index (the NIC carrying most
/// traffic) from `/proc/net/route` + `/sys/class/net/<iface>/ifindex`, for
/// binding the capture to one interface. Returns `0` (AF_PACKET "any") when no
/// default route is found — acceptable only because the unprivileged open fails
/// regardless; the privileged path expects a routable host.
pub(crate) fn default_route_iface_index(proc_root: &Path) -> u32 {
    let Some(iface) = default_route_iface(proc_root) else {
        return 0;
    };
    std::fs::read_to_string(proc_root.join("sys/class/net").join(&iface).join("ifindex"))
        .ok()
        .and_then(|raw| raw.trim().parse().ok())
        .unwrap_or(0)
}

/// Classify an `AF_PACKET` open failure: a `PermissionDenied` becomes
/// [`NetworkAccountingFailure::RequiresEscalation`] when the gate confirms
/// [`EscalationFeature::PerProcessNet`] is escalatable (the unprivileged
/// default — offer the prompt), else [`PermissionDenied`]. Any other open
/// error (interface gone, resource limits) is not escalatable → [`Unavailable`].
/// Mirrors `classify_rapl_io` / `classify_smbios_io`.
fn classify_afpacket_io(error: &io::Error) -> NetworkAccountingFailure {
    use taskmanager_escalation::{
        EscalationAvailability, EscalationFeature, PrivilegeGate, UnprivilegedGate,
    };
    let denied = error.kind() == io::ErrorKind::PermissionDenied;
    if denied
        && matches!(
            UnprivilegedGate.probe(EscalationFeature::PerProcessNet),
            EscalationAvailability::RequiresEscalation(_)
        )
    {
        NetworkAccountingFailure::RequiresEscalation
    } else if denied {
        NetworkAccountingFailure::PermissionDenied
    } else {
        NetworkAccountingFailure::Unavailable
    }
}

fn default_route_iface(proc_root: &Path) -> Option<String> {
    let table = std::fs::read_to_string(proc_root.join("net/route")).ok()?;
    for line in table.lines().skip(1) {
        let mut cols = line.split_whitespace();
        let iface = cols.next()?;
        // Destination 00000000 is the default route. Skip loopback — `lo` can
        // carry a 00000000/127.0.0.1 entry but never the routable default we
        // want to capture.
        if iface == "lo" {
            continue;
        }
        if cols.next().is_some_and(|dest| dest == "00000000") {
            return Some(iface.to_owned());
        }
    }
    None
}

/// The four host INET socket tables with their transport/family metadata, in the
/// same order [`super::network`] reads them.
const INET_SOCKET_TABLES: [(&str, ConnectionTransport, ConnectionAddressFamily); 4] = [
    (
        "tcp",
        ConnectionTransport::Tcp,
        ConnectionAddressFamily::Ipv4,
    ),
    (
        "tcp6",
        ConnectionTransport::Tcp,
        ConnectionAddressFamily::Ipv6,
    ),
    (
        "udp",
        ConnectionTransport::Udp,
        ConnectionAddressFamily::Ipv4,
    ),
    (
        "udp6",
        ConnectionTransport::Udp,
        ConnectionAddressFamily::Ipv6,
    ),
];

fn transport_proto(transport: &ConnectionTransport) -> Option<u8> {
    match transport {
        ConnectionTransport::Tcp => Some(TCP),
        ConnectionTransport::Udp => Some(UDP),
        _ => None,
    }
}

fn endpoint_addr(endpoint: &ConnectionEndpoint) -> Option<SocketAddr> {
    match endpoint {
        ConnectionEndpoint::Ip(addr) => Some(*addr),
        // Local/Opaque/Unspecified endpoints are not IP flows — never attributable.
        _ => None,
    }
}

fn sort_pair(a: SocketAddr, b: SocketAddr) -> (SocketAddr, SocketAddr) {
    if a <= b { (a, b) } else { (b, a) }
}

fn entry_pid(entry: &std::fs::DirEntry) -> Option<u32> {
    entry.file_name().to_string_lossy().parse().ok()
}

fn socket_inode(target: &str) -> Option<u64> {
    target
        .strip_prefix("socket:[")?
        .strip_suffix(']')?
        .parse()
        .ok()
}

#[cfg(test)]
#[path = "../../../../tests/headless/linux_engine_process_telemetry_net_accounting_tests.rs"]
mod tests;
