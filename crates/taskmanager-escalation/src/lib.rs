//! Per-feature privilege-escalation seam for TaskForest (ADR-023, permission-
//! model Boundary 2 operationalized).
//!
//! The product runs **unprivileged by default on every platform** (Boundary 2
//! of `docs/PERMISSION_MODEL.md`): the binary never launches elevated and
//! carries no blanket capability (`setcap`/`setuid` are forbidden on the main
//! binary). Some telemetry/control domains still need a capability or syscall
//! an unprivileged user lacks — the escalation column of Boundary 3. Those
//! features are reached through ONE cross-platform seam: a small privileged
//! helper invoked via the OS-native escalation prompt (polkit `.policy` +
//! `pkexec` on Linux; Windows UAC via the [`uac`] transport facts with the
//! runas call group in `taskmanager-windows-api` and the driver in
//! `taskmanager-platform-windows`; macOS authorization typed in
//! [`authorization`] with the Security-framework crossing unwired pending a
//! signed-helper ADR) that performs ONLY the privileged op and returns safe
//! typed data. Unwired native crossings fail closed as typed `Unsupported`
//! and never spawn a normal child while claiming elevation.
//!
//! This crate is the FOUNDATION of that framework — the seam types plus the
//! honest default gate. It contains:
//! * [`EscalationFeature`] — the escalation-column features from Boundary 3;
//! * [`EscalationAvailability`] — the typed probe result (Available /
//!   RequiresEscalation / Denied), never a fabricated number or zero;
//! * [`PrivilegeGate`] — the one-method seam a feature asks before touching a
//!   privileged path; and
//! * [`UnprivilegedGate`] — the honest default: `RequiresEscalation` for every
//!   variant, because a freshly started TaskForest has escalated nothing yet.
//!
//! The actual OS-native prompt invocations for the privileged helpers live in
//! the [`polkit`] submodule (the operational crossing): it drives `pkexec` and
//! parses each helper's typed JSON contract (perf PMU, net launcher, foreign
//! process control, SMBIOS memory, RAPL package power). The SEAM in THIS file
//! stays pure — no capability grant, no elevated-process launch — so the
//! workspace gate that
//! requires `lib.rs` itself to be free of `pkexec`/`Command::new` keeps
//! holding. The privileged helper binary is owned by a separate boundary
//! artifact; see ADR-023. The Windows UAC transport seam ([`uac`], ADR-035)
//! supplies the typed transport facts, the pure fact→outcome mappings, and
//! the stage-2 pure launch layer; because this crate is zero-dependency
//! safe Rust, the raw runas call group lives in the audited
//! `taskmanager-windows-api` boundary and the production driver in
//! `taskmanager-platform-windows`, which feeds those facts back through
//! [`uac::invoke_uac_foreign_process_control_with`].
//!
//! Honesty red line: missing/unavailable data is reported as a typed
//! [`EscalationAvailability`] variant, never a fabricated value.

#![forbid(unsafe_code)]

/// The operational escalation crossing for the Intel PMU helper: the real
/// [`PolkitGate`](polkit::PolkitGate) probe plus
/// [`invoke_perf_helper`](polkit::invoke_perf_helper), which drives `pkexec` and
/// parses the helper's shared JSON contract. Kept in a sibling module so this
/// `lib.rs` remains the pure, zero-dependency, capability-free seam.
pub mod polkit;

/// The Windows UAC transport seam for foreign-process control (ADR-035):
/// the typed transport-fact vocabulary, the pure fact→outcome mapping, the
/// install-fact readiness probe, and the stage-2 pure launch layer (helper
/// command-line builder + one-shot reply-channel naming). The runas call
/// group lives in the audited `taskmanager-windows-api` boundary and the
/// production driver in `taskmanager-platform-windows`; both feed this
/// mapping through [`uac::invoke_uac_foreign_process_control_with`].
pub mod uac;

/// The macOS native-authorization transport seam for foreign-process
/// control: typed transport facts, the pure fact→outcome mapping, the
/// install-fact readiness probe, and the injectable transport trait. The
/// Security-framework crossing itself stays unwired (`Unsupported`) until a
/// signed privileged-helper ADR exists — `AuthorizationExecuteWithPrivileges`
/// is deprecated and `osascript` is a command-interpreter path.
pub mod authorization;

/// The escalation-column features from permission-model Boundary 3.
///
/// Each variant is a telemetry/control domain that an unprivileged user cannot
/// obtain directly: it needs a capability or syscall the running user lacks.
/// Reaching the real data requires the per-feature OS-native escalation prompt
/// (Boundary 2), routed through the privileged helper (ADR-023).
///
/// The set mirrors the "Requires escalation" rows of the Boundary 3 table in
/// `docs/PERMISSION_MODEL.md`; a new escalation-column domain must land here
/// AND there so the UI can offer exactly that one prompt.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EscalationFeature {
    /// Intel i915 per-engine utilization via the `perf_event_open` PMU — the
    /// sysfs `busy` node is absent on mainline i915. Denied under a restrictive
    /// `perf_event_paranoid` (e.g. paranoid = 2). The audited boundary crate
    /// (`taskmanager-perf-ioctl`, ADR-022) opens the fd; this feature gates
    /// whether the unprivileged app may use it at all.
    IntelPmu,
    /// Per-process network rates via `AF_PACKET` raw sockets (`CAP_NET_RAW`).
    /// The audited boundary crate (`taskmanager-afpacket`, ADR-024) probes the
    /// socket at construction and degrades to a typed escalatable denial without
    /// the capability. eBPF was removed by ADR-021 as too large a trust root;
    /// `AF_PACKET` is the smaller carve-out. The `CAP_NET_RAW` launcher
    /// (`taskmanager-net-launcher`, SCM_RIGHTS fd-passing, ADR-024/025) obtains
    /// the fd via the OS-native prompt and is consumed through the capability
    /// lane + runtime UI swap; [`PolkitGate`](polkit::PolkitGate) probes its
    /// installed state (prompt available vs `HelperUnavailable`).
    PerProcessNet,
    /// ATA/SATA SMART via `smartctl /dev/sd*`, which frequently needs root.
    /// NVMe-only SMART stays in the direct column (`/sys/class/nvme`).
    AtaSmart,
    /// Process control (kill / nice / affinity / resource limits) of
    /// foreign-uid processes — `CAP_KILL` / `CAP_SYS_PTRACE` / `setpriority`
    /// for other users. Same-uid control stays direct.
    ForeignProcessControl,
    /// System service start/stop — systemd polkit auth, or OpenRC which needs
    /// root. User-unit control and service/session READ stay direct.
    SystemServiceControl,
    /// Memory speed / slot inventory via raw SMBIOS type-17 records. The direct
    /// `/sys/firmware/dmi/entries/17-*/raw` parse is correct but those `raw`
    /// nodes are mode 0400 (root-only) on mainline kernels, so the unprivileged
    /// read fails and the metric renders as unavailable until the user escalates
    /// (the `taskforest-smbios-helper` binary reads the same files as root; the
    /// crossing is [`polkit::invoke_smbios_helper`]). Mirrors `IntelPmu`:
    /// classify the denial as escalatable, do not fetch from the refresh tick.
    MemorySmbios,
    /// CPU package power via RAPL `/sys/class/powercap/intel-rapl:*/energy_uj`.
    /// Same root-only (0400) situation as `MemorySmbios`; classify-and-escalate
    /// through the `taskforest-rapl-helper` binary
    /// ([`polkit::invoke_rapl_helper`]).
    PackagePowerRapl,
    /// CPU MSR readouts (package temperature, P-state multipliers, P-state
    /// core voltage) via the root-only (0600) `/dev/cpu/*/msr` nodes. MSR
    /// reads are plain file I/O (open + pread), so the
    /// `taskforest-msr-helper` binary is safe Rust with no new `unsafe`
    /// trust root (ADR-048); the crossing is
    /// [`polkit::invoke_msr_helper`]. Unimplemented registers and the
    /// unverifiable base clock decode to honest per-field nulls.
    CpuMsr,
}

impl EscalationFeature {
    /// Every escalation-column feature, in Boundary-3 order.
    ///
    /// Single source of truth shared by the crate's own unit tests and the
    /// workspace architecture gate, so the enum variants, the
    /// `docs/PERMISSION_MODEL.md` Boundary-3 table, and every test that must
    /// cover "all escalation features" can never again drift to different
    /// counts (a prior workspace gate hardcoded 5 of 7 variants and silently
    /// missed two). Adding a variant requires appending it here too.
    pub const ALL: [EscalationFeature; 8] = [
        EscalationFeature::IntelPmu,
        EscalationFeature::PerProcessNet,
        EscalationFeature::AtaSmart,
        EscalationFeature::ForeignProcessControl,
        EscalationFeature::SystemServiceControl,
        EscalationFeature::MemorySmbios,
        EscalationFeature::PackagePowerRapl,
        EscalationFeature::CpuMsr,
    ];
}

/// Why an escalation feature is denied, in typed form.
///
/// Carried by [`EscalationAvailability::Denied`] so the UI can distinguish
/// "this host will never support it" from "the user said no" from "the helper
/// is missing" without any string parsing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EscalationDenialReason {
    /// The platform, kernel, or init system does not expose this capability at
    /// all (e.g. no Intel GPU, cgroup-v1 for a v2-only feature, OpenRC for a
    /// systemd-only op). Permanent for this host.
    Unsupported,
    /// The user dismissed or refused the OS-native escalation prompt, or the
    /// policy disallows it for this session. Retryable by asking again.
    PermissionDenied,
    /// The OS-native authorization broker did not complete the request even
    /// though the installed crossing is present. On observed polkit versions
    /// `pkexec` 127 covers both a canceled dialog and broker/policy failure;
    /// callers must use neutral "authorization did not complete" copy rather
    /// than inventing either cause.
    AuthorizationUnavailable,
    /// The privileged helper is not installed, not registered with the OS
    /// policy agent, or otherwise unreachable on this host.
    HelperUnavailable,
    /// The installed helper returned without one valid contract message.
    /// This is an implementation/protocol fault, not a missing install and
    /// not evidence that the user rejected authorization.
    HelperProtocolViolation,
}

/// The result of probing one escalation feature under the current privilege
/// level.
///
/// Honesty red line: an unavailable feature is reported as a typed variant,
/// never a fabricated value or a silent zero. [`EscalationAvailability::
/// RequiresEscalation`] is the honest default for an unprivileged process that
/// has not yet been escalated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EscalationAvailability {
    /// The feature is usable right now without further privilege — the helper
    /// already ran and returned data this session, or the host grants the
    /// capability directly to the unprivileged process.
    Available,
    /// The feature needs privilege and the caller has not yet escalated; route
    /// through the OS-native prompt (Boundary 2). Carries the feature so the UI
    /// can offer exactly that one escalation, nothing blanket.
    RequiresEscalation(EscalationFeature),
    /// The feature is permanently or contextually denied: restrictive host,
    /// unsupported platform, helper missing, or the user refused the prompt.
    /// Never fabricated. `reason` is typed so callers can react without parsing.
    Denied {
        /// Typed reason the feature cannot be used on this host / session.
        reason: EscalationDenialReason,
    },
}

/// The per-feature privilege-escalation seam (permission-model Boundary 2,
/// operationalized).
///
/// A [`PrivilegeGate`] answers one question for a single feature: is it usable
/// right now, does it need escalation, or is it denied? The default
/// [`UnprivilegedGate`] reports [`EscalationAvailability::RequiresEscalation`]
/// for everything, encoding the product principle that the app runs
/// unprivileged and nothing is escalated until the user actively uses a
/// feature.
///
/// This trait is the side-effect-free probe seam. Linux's operational polkit
/// crossing lives in [`crate::polkit`]; other platform transports remain
/// unwired and therefore cannot claim availability.
pub trait PrivilegeGate {
    /// Probe one feature's availability under the current privilege level.
    ///
    /// Implementations must be side-effect-free with respect to privilege:
    /// probing never grants a capability and never launches an elevated
    /// process. Escalation itself is a separate, explicit invocation.
    fn probe(&self, feature: EscalationFeature) -> EscalationAvailability;
}

/// The honest default gate: the app runs unprivileged, so every escalation
/// feature reports [`EscalationAvailability::RequiresEscalation`] until a real
/// escalation path is wired.
///
/// Constructing this type never grants any capability and never launches an
/// elevated process. It is the value a freshly started, unprivileged TaskForest
/// uses for every escalation-column feature. Wiring a real gate (one that
/// talks to the privileged helper / OS prompt) is a follow-up tracked in
/// ADR-023 and the integration notes — NOT part of this foundation.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UnprivilegedGate;

impl PrivilegeGate for UnprivilegedGate {
    fn probe(&self, feature: EscalationFeature) -> EscalationAvailability {
        // The app starts unprivileged and has escalated nothing yet, so every
        // escalation-column feature is honestly reported as requiring
        // escalation — never Available (would fabricate access) and never
        // Denied (would hide a real opportunity behind a hard refusal).
        EscalationAvailability::RequiresEscalation(feature)
    }
}

#[cfg(test)]
#[path = "../tests/headless/escalation_lib.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/common/test_support.rs"]
pub(crate) mod test_support;
