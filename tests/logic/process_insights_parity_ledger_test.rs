//! Process-insights three-frontend parity ledger — a fail-silent gate, not a
//! behavior acceptance test.
//!
//! The Process Properties "Insights" surface spans five independently
//! scheduled domains (threads / network / gpu / resources+cgroup / isolation,
//! plus the open-files enrichment — see `docs/CORE_100_CLOSURE.md`
//! CORE-PROCESS-INSIGHTS-01). The three frontends drifted to different depths
//! with no explicit record, so gaps could exist silently. This ledger makes
//! every (facet, frontend) combination an explicit status declaration:
//!
//! - `Ready` — the facet renders in that frontend (with its honesty contract),
//!   and carries a non-empty evidence signpost.
//! - `Partial(reason)` — the facet renders but with a stated capability gap.
//! - `Missing` — the facet does not render there at all.
//!
//! This file is an honest status snapshot, taken from a code scan on
//! 2026-08-20 of:
//! - GPUI: `crates/taskmanager-gpui/src/gpui_app/process_insights.rs`,
//!   `.../process_insights/{view.rs, view/, worker.rs}` and
//!   `.../root/process_insights_ui.rs`
//! - Iced: `crates/taskmanager-iced/src/ui/insights.rs` (+ `insights/tests.rs`)
//! - TUI: `crates/taskmanager-tui/src/ui/process_details/insights.rs` (+ the
//!   process-properties modal that reuses it)
//!
//! The `evidence` strings are signposts for human verification (file:line or
//! test name at snapshot time); this test never reads source files and never
//! asserts on source text — behavior acceptance stays with each frontend's
//! own tests and pixel evidence. The gate below only enforces ledger
//! integrity: total (facet × frontend) coverage with exactly one entry per
//! combination, evidence on `Ready`, a reason on `Partial`, and an
//! anti-regression ceiling on the `Missing` count.

/// The three equal product frontends that render process insights.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Frontend {
    Gpui,
    Iced,
    Tui,
}

impl Frontend {
    const ALL: [Frontend; 3] = [Frontend::Gpui, Frontend::Iced, Frontend::Tui];
}

/// The facet union across the three frontends, derived from the code scan
/// (not invented): six per-facet user-visible capabilities plus the
/// cross-cutting axes every frontend must decide about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Facet {
    // Network domain.
    /// Received/Sent throughput rates (bytes per second).
    NetworkThroughput,
    /// Connection list rows (transport + local → remote).
    NetworkConnections,
    /// Typed RequiresEscalation reason plus the affordance to enable capture.
    NetworkEscalation,
    // GPU domain.
    /// Per-device rollup (device id, utilization %, VRAM).
    GpuDevices,
    /// Per-engine breakdown (name, rate, cumulative busy time or cycles).
    GpuEngines,
    // Resources / cgroup domain.
    /// Memory usage / limit pair (unlimited renders honestly).
    ResourcesMemory,
    /// CPU quota as a percentage of one period.
    ResourcesCpuQuota,
    /// PID count / limit pair.
    ResourcesPidLimits,
    /// Resource-group (cgroup) native locator.
    ResourcesCgroupLocator,
    // Isolation domain.
    /// Container/sandbox kind (Docker/…/Host process).
    IsolationKind,
    /// Container id.
    IsolationContainerId,
    /// Sandboxed yes/no flag.
    IsolationSandboxed,
    // Threads domain.
    /// Per-thread rows (tid, name, state, cpu time, cpu %).
    ThreadsList,
    // Open-files enrichment.
    /// fd → target rows with the unreadable marker and count header.
    OpenFilesList,
    // Cross-cutting axes.
    /// Frozen-identity request with revision/target-correlated application.
    RequestLifecycle,
    /// Honest pending state before the first projection arrives.
    LoadingState,
    /// Typed per-facet unavailable reasons (never Debug formatting).
    TypedUnavailable,
    /// Partial display: sibling facets render while one is unavailable.
    PartialDisplay,
    /// Value-level gap honesty: explicit dash, never a fabricated zero.
    GapHonesty,
    /// Deterministic capture fixture / pixel-evidence scene for the surface.
    CaptureEvidence,
}

impl Facet {
    const ALL: [Facet; 20] = [
        Facet::NetworkThroughput,
        Facet::NetworkConnections,
        Facet::NetworkEscalation,
        Facet::GpuDevices,
        Facet::GpuEngines,
        Facet::ResourcesMemory,
        Facet::ResourcesCpuQuota,
        Facet::ResourcesPidLimits,
        Facet::ResourcesCgroupLocator,
        Facet::IsolationKind,
        Facet::IsolationContainerId,
        Facet::IsolationSandboxed,
        Facet::ThreadsList,
        Facet::OpenFilesList,
        Facet::RequestLifecycle,
        Facet::LoadingState,
        Facet::TypedUnavailable,
        Facet::PartialDisplay,
        Facet::GapHonesty,
        Facet::CaptureEvidence,
    ];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Status {
    Ready,
    Partial,
    Missing,
}

#[derive(Debug)]
struct LedgerEntry {
    facet: Facet,
    frontend: Frontend,
    status: Status,
    /// Required non-empty for `Partial` (the honest capability gap).
    reason: &'static str,
    /// Human-verification signpost (file:line or test name at snapshot
    /// time). Required non-empty for `Ready`. Never checked against the
    /// filesystem — that would be a source-text acceptance red line.
    evidence: &'static str,
}

/// Snapshot date: 2026-08-20 (code scan of the three insight surfaces).
const LEDGER: [LedgerEntry; 60] = [
    // ---- GPUI -------------------------------------------------------------
    LedgerEntry {
        facet: Facet::NetworkThroughput,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "crates/taskmanager-gpui/src/gpui_app/process_insights/view.rs:335-347 network_card Received/Sent metric rows; format_rate renders the unavailable label on None",
    },
    LedgerEntry {
        facet: Facet::NetworkConnections,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:294-334 scrollable bounded list (cap 200 + '… N more'); format_connection view.rs:537",
    },
    LedgerEntry {
        facet: Facet::NetworkEscalation,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:355-394 escalation_row pill; process_insights/tests.rs:188 escalation_pill_renders_for_requires_escalation_and_submits_on_click",
    },
    LedgerEntry {
        facet: Facet::GpuDevices,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:396-446 gpu_card device_id / utilization % / VRAM per device",
    },
    LedgerEntry {
        facet: Facet::GpuEngines,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view/gpu_engines.rs:73 card, :49 format_engine_line rate + cumulative ns-or-cycles",
    },
    LedgerEntry {
        facet: Facet::ResourcesMemory,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:455-461 memory usage / limit pair with Unlimited label",
    },
    LedgerEntry {
        facet: Facet::ResourcesCpuQuota,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:462-471 quota/period percentage",
    },
    LedgerEntry {
        facet: Facet::ResourcesPidLimits,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:472-478 pid count / limit pair",
    },
    LedgerEntry {
        facet: Facet::ResourcesCgroupLocator,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:479-488 first resource-group native_locator row",
    },
    LedgerEntry {
        facet: Facet::IsolationKind,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:498-502 + 576-587 isolation_label (all eight kinds + host fallback)",
    },
    LedgerEntry {
        facet: Facet::IsolationContainerId,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:511-513 container-id row when present",
    },
    LedgerEntry {
        facet: Facet::IsolationSandboxed,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:503-510 yes / no / unknown row",
    },
    LedgerEntry {
        facet: Facet::ThreadsList,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view/threads.rs:54-127 five-column bounded card; cap contract view.rs:729-751 capped_card_rows tests",
    },
    LedgerEntry {
        facet: Facet::OpenFilesList,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view/open_files.rs:39-109 fd → target list, unreadable count in header",
    },
    LedgerEntry {
        facet: Facet::RequestLifecycle,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "root/process_insights_ui.rs:98-166 frozen-identity submit from the 200 ms tick; :168-194 revision+pid-correlated apply rejects stale projections",
    },
    LedgerEntry {
        facet: Facet::LoadingState,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "process_insights.rs:38-42 ProcessInsightsState::Loading; view.rs:170-175 loading panel",
    },
    LedgerEntry {
        facet: Facet::TypedUnavailable,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "view.rs:517-535 status_label / error_label typed kinds; process_insights.rs:21-28 five error kinds",
    },
    LedgerEntry {
        facet: Facet::PartialDisplay,
        frontend: Frontend::Gpui,
        status: Status::Partial,
        reason: "per-facet degraded cards render only inside a Ready snapshot; the whole dialog stays Loading until the four core facets reach terminal states, and total failure folds into one typed Error panel (coarser than the per-facet streaming of iced/tui)",
        evidence: "root/process_insights_ui.rs:168-194; application platform/process_insights_projection/terminal.rs (Unavailable → typed degraded snapshot inside complete_snapshot)",
    },
    LedgerEntry {
        facet: Facet::GapHonesty,
        frontend: Frontend::Gpui,
        status: Status::Ready,
        reason: "",
        evidence: "threads.rs:176 format_keeps_missing_cpu_time_honest; gpu_engines.rs:200 format_keeps_cold_start_gap_honest; open_files.rs:159 format_keeps_unreadable_target_honest",
    },
    LedgerEntry {
        facet: Facet::CaptureEvidence,
        frontend: Frontend::Gpui,
        status: Status::Partial,
        reason: "deterministic capture fixture plus headless window-draw and GUI behavior tests exist, but no dedicated insights scene in the canonical capture matrix",
        evidence: "view.rs:590 process_insights_capture_fixture; process_insights/tests.rs:165 capture_fixture_renders_at_reference_and_compact_sizes; tests/gui/keyboard_behavior.rs:386",
    },
    // ---- Iced -------------------------------------------------------------
    LedgerEntry {
        facet: Facet::NetworkThroughput,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "crates/taskmanager-iced/src/ui/insights.rs:94-110 Received/Sent kv rows with dash on None",
    },
    LedgerEntry {
        facet: Facet::NetworkConnections,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:112-127 bounded list (MAX_FACET_ROWS=8 + '… +N more'), count in heading",
    },
    LedgerEntry {
        facet: Facet::NetworkEscalation,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:131-157 escalation pill → Message::RequestProcessNetworkEscalation (app/update.rs:390)",
    },
    LedgerEntry {
        facet: Facet::GpuDevices,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:180-224 device_id / utilization % / VRAM rows",
    },
    LedgerEntry {
        facet: Facet::GpuEngines,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:470-524 section, :640-659 format_engine_usage rate + cumulative ns-or-cycles",
    },
    LedgerEntry {
        facet: Facet::ResourcesMemory,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:245-255 usage / limit with ∞ for unlimited",
    },
    LedgerEntry {
        facet: Facet::ResourcesCpuQuota,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:256-272 quota percentage with ∞ fallback",
    },
    LedgerEntry {
        facet: Facet::ResourcesPidLimits,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:276-286 pid count / limit with ∞ fallback",
    },
    LedgerEntry {
        facet: Facet::ResourcesCgroupLocator,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:287-297 first non-empty resource-group locator row",
    },
    LedgerEntry {
        facet: Facet::IsolationKind,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:325-340 all eight kinds + host-process fallback",
    },
    LedgerEntry {
        facet: Facet::IsolationContainerId,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:341-349 container-id row when non-empty",
    },
    LedgerEntry {
        facet: Facet::IsolationSandboxed,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:350-356 yes / no row",
    },
    LedgerEntry {
        facet: Facet::ThreadsList,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:366-407 column-header row + bounded thread rows with '+N more'",
    },
    LedgerEntry {
        facet: Facet::OpenFilesList,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:413-464 count (+ unreadable) heading, fd → target rows",
    },
    LedgerEntry {
        facet: Facet::RequestLifecycle,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "shell request path: taskmanager-shell/src/app.rs:932 request_process_insights freezes identity, app/effect_dispatch.rs:117 submits; iced app/refresh.rs:27 + app/update.rs:329 drive it; insights.rs:48-52 filters by frozen target",
    },
    LedgerEntry {
        facet: Facet::LoadingState,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:79-83 per-facet 'collecting…' hint; block always renders (:43-66)",
    },
    LedgerEntry {
        facet: Facet::TypedUnavailable,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:665-678 facet_unavailable_text; insights/tests.rs:91 facet_unavailable_text_maps_typed_reasons",
    },
    LedgerEntry {
        facet: Facet::PartialDisplay,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:43-66 every facet renders its own Pending / Unavailable / Current independently",
    },
    LedgerEntry {
        facet: Facet::GapHonesty,
        frontend: Frontend::Iced,
        status: Status::Ready,
        reason: "",
        evidence: "insights/tests.rs:12 thread_cpu_helpers_keep_a_missing_value_honest, :60 engine_usage_keeps_a_cold_start_gap_honest, :37 open_file_row_marks_an_unreadable_target_not_blank",
    },
    LedgerEntry {
        facet: Facet::CaptureEvidence,
        frontend: Frontend::Iced,
        status: Status::Missing,
        reason: "no insights scene in the canonical Iced capture matrix (Applications + Performance + CPU compact); only helper-level unit tests exist",
        evidence: "insights/tests.rs honesty units; scripts/validate_iced_matrix.py scene list",
    },
    // ---- TUI --------------------------------------------------------------
    LedgerEntry {
        facet: Facet::NetworkThroughput,
        frontend: Frontend::Tui,
        status: Status::Missing,
        reason: "the Current network branch renders connection count/endpoints only; rx/tx rates never drawn",
        evidence: "crates/taskmanager-tui/src/ui/process_details/insights.rs:90-110",
    },
    LedgerEntry {
        facet: Facet::NetworkConnections,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:90-110 connection count + first 3 endpoints + honest '…'",
    },
    LedgerEntry {
        facet: Facet::NetworkEscalation,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:74-88 typed hint line; runtime/modals.rs:168 'e' key gated by network_requires_escalation (insights.rs:24)",
    },
    LedgerEntry {
        facet: Facet::GpuDevices,
        frontend: Frontend::Tui,
        status: Status::Partial,
        reason: "per-device utilization % + VRAM render but no device id; two-device preview bound",
        evidence: "insights.rs:113-135",
    },
    LedgerEntry {
        facet: Facet::GpuEngines,
        frontend: Frontend::Tui,
        status: Status::Partial,
        reason: "engine name + usage % only; cumulative busy time / cycle counter not rendered",
        evidence: "insights.rs:139-151 + format_engine_usage_line :408-413",
    },
    LedgerEntry {
        facet: Facet::ResourcesMemory,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:157-162 limit_value usage / limit with ∞",
    },
    LedgerEntry {
        facet: Facet::ResourcesCpuQuota,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:163-169 quota row (limit_value)",
    },
    LedgerEntry {
        facet: Facet::ResourcesPidLimits,
        frontend: Frontend::Tui,
        status: Status::Missing,
        reason: "no pid count / limit row (resources branch renders memory + quota only)",
        evidence: "insights.rs:153-171",
    },
    LedgerEntry {
        facet: Facet::ResourcesCgroupLocator,
        frontend: Frontend::Tui,
        status: Status::Missing,
        reason: "no resource-group / cgroup locator row",
        evidence: "insights.rs:153-171",
    },
    LedgerEntry {
        facet: Facet::IsolationKind,
        frontend: Frontend::Tui,
        status: Status::Missing,
        reason: "isolation branch renders container id only; kind (Docker/…/Host) not rendered",
        evidence: "insights.rs:173-187",
    },
    LedgerEntry {
        facet: Facet::IsolationContainerId,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:173-187 container-id row with honest dash",
    },
    LedgerEntry {
        facet: Facet::IsolationSandboxed,
        frontend: Frontend::Tui,
        status: Status::Missing,
        reason: "sandboxed flag not rendered",
        evidence: "insights.rs:173-187",
    },
    LedgerEntry {
        facet: Facet::ThreadsList,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:304-342 bounded preview + column header; tests thread_preview_renders_header_rows_and_ellipsis (:517)",
    },
    LedgerEntry {
        facet: Facet::OpenFilesList,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:360-402 count (+ unreadable) header + fd → target rows; test :592",
    },
    LedgerEntry {
        facet: Facet::RequestLifecycle,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "selection.rs:375 refresh_selected_process_insights (frozen-identity dedupe on the projection revision); insights.rs:60-65 target filter; runtime/seam.rs:407 tick path",
    },
    LedgerEntry {
        facet: Facet::LoadingState,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:54-65 honest 'collecting' line (missing or mismatched projection)",
    },
    LedgerEntry {
        facet: Facet::TypedUnavailable,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:209-228 insight_unavailable typed mapping (G-04b, never Debug formatting)",
    },
    LedgerEntry {
        facet: Facet::PartialDisplay,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "insights.rs:48-201 per-facet Pending / Unavailable / Current; one renderer shared by the detail panel and the modal (process_properties.rs:143)",
    },
    LedgerEntry {
        facet: Facet::GapHonesty,
        frontend: Frontend::Tui,
        status: Status::Ready,
        reason: "",
        evidence: "in-file tests format_thread_row_keeps_missing_cpu_honest (:447), format_engine_usage_line_keeps_cold_start_honest (:503), format_open_file_row_keeps_unreadable_target_honest (:481)",
    },
    LedgerEntry {
        facet: Facet::CaptureEvidence,
        frontend: Frontend::Tui,
        status: Status::Missing,
        reason: "TestBackend unit renders only; no insights-modal scene in scripts/capture-tui.sh",
        evidence: "insights.rs in-file tests (:434 render_text harness)",
    },
];

/// Anti-regression baseline for the `Missing` count. This is a direction
/// guard (the gap may only shrink), NOT a behavior proof — a `Missing` count
/// above this number means a frontend silently lost a facet and must be an
/// explicit ledger edit instead. Update it downward only when a gap closes.
const MISSING_BASELINE: usize = 7;

/// Every (facet, frontend) combination must be declared exactly once: the
/// facet union times the frontend set is the complete grid, so a new facet or
/// frontend that is not ledgered fails here instead of staying silent.
#[test]
fn every_facet_frontend_combination_has_exactly_one_entry() {
    assert_eq!(
        LEDGER.len(),
        Facet::ALL.len() * Frontend::ALL.len(),
        "ledger size must equal the facet × frontend grid"
    );
    for facet in Facet::ALL {
        for frontend in Frontend::ALL {
            let matches = LEDGER
                .iter()
                .filter(|entry| entry.facet == facet && entry.frontend == frontend)
                .count();
            assert_eq!(
                matches, 1,
                "facet {facet:?} on {frontend:?} must have exactly one ledger entry, got {matches}"
            );
        }
    }
}

/// A `Ready` claim without a signpost cannot be human-verified, so every
/// Ready entry carries non-empty evidence (a file:line or test name at
/// snapshot time — the gate never reads those files).
#[test]
fn ready_entries_carry_evidence() {
    for entry in LEDGER.iter().filter(|entry| entry.status == Status::Ready) {
        assert!(
            !entry.evidence.trim().is_empty(),
            "Ready entry {entry:?} must carry evidence for human verification"
        );
    }
}

/// A `Partial` claim must state the honest capability gap it names.
#[test]
fn partial_entries_state_their_reason() {
    for entry in LEDGER
        .iter()
        .filter(|entry| entry.status == Status::Partial)
    {
        assert!(
            !entry.reason.trim().is_empty(),
            "Partial entry {entry:?} must state its reason"
        );
    }
}

/// Direction guard: the explicit `Missing` count may not grow. This is an
/// anti-regression ceiling on the ledger itself, not a behavior assertion —
/// closing a gap requires editing the entry (and lowering this baseline).
#[test]
fn missing_count_never_grows() {
    let missing = LEDGER
        .iter()
        .filter(|entry| entry.status == Status::Missing)
        .count();
    assert!(
        missing <= MISSING_BASELINE,
        "process-insights parity regressed: {missing} Missing entries (baseline {MISSING_BASELINE}); \
         a frontend silently lost a facet — restore it or make the loss an explicit ledger decision"
    );
}
