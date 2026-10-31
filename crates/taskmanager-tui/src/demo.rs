//! Deterministic full-surface demo frame construction: the `demo()` impl plus
//! its two honest fixtures (directory-usage + boot-evidence). Extracted from
//! `lib.rs` to keep the crate root under the source line budget; behavior is
//! unchanged — `demo_app()` stays reachable at `crate::demo_app` via a
//! `pub use` in `lib.rs`.

use crate::TuiApp;
use taskforest_core::core::device_state::DeviceState;
use taskforest_core::core::metrics::ScalarObservation;
use taskforest_core::core::process_telemetry::{
    ContainerQuota, ContainerRollup, ContainerSummary, IsolationKind,
};
use taskforest_core::core::time::{LocalTimeRules, LocalTimeRulesObservation};
use taskmanager_shell::fixture::{ProjectionSeedFact, seed_projection_fact};

pub(crate) mod capture;
pub(crate) mod capture_overrides;
pub(crate) use capture_overrides::persisted_history_capture_requested;
pub(crate) mod fixtures;
mod history;
mod power;
use fixtures::{demo_boot_evidence, demo_directory_usage, seed_demo_npu_inventory};
use history::seed_demo_history;

impl TuiApp {
    /// A deterministic full-surface demo frame: the shared demo snapshot plus
    /// seeded containers. It has no configuration capability, so demo-mode
    /// settings can update local presentation but never touch a host file.
    #[must_use]
    pub fn demo() -> Self {
        use taskmanager_shell::demo_app;
        let mut app = Self::from_shell(demo_app());
        app.local_time_rules = LocalTimeRulesObservation::current(LocalTimeRules::utc(), 0);
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::Containers(Some(ContainerRollup {
                state: DeviceState::healthy(1_785_292_800_000),
                containers: vec![
                    ContainerSummary {
                        id: "/docker/abc123".into(),
                        name: "postgres".into(),
                        runtime: Some(IsolationKind::Docker),
                        cgroup_path: "/docker/abc123".into(),
                        cpu_percentage: ScalarObservation::available(12.5, 1_785_292_800_000),
                        memory_bytes: ScalarObservation::available(
                            68 * 1024 * 1024 + 512 * 1024,
                            1_785_292_800_000,
                        ),
                        member_pids: vec![4201, 4202],
                        // The demo data declares no cgroup ceiling.
                        quota: ContainerQuota::not_declared(),
                    },
                    ContainerSummary {
                        id: "/docker/def456".into(),
                        name: "redis".into(),
                        runtime: Some(IsolationKind::Docker),
                        cgroup_path: "/docker/def456".into(),
                        cpu_percentage: ScalarObservation::available(3.1, 1_785_292_800_000),
                        memory_bytes: ScalarObservation::available(
                            24 * 1024 * 1024,
                            1_785_292_800_000,
                        ),
                        member_pids: vec![4301],
                        // The demo data declares no cgroup ceiling.
                        quota: ContainerQuota::not_declared(),
                    },
                ],
            })),
        );
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::StartupBootEvidence(Some(demo_boot_evidence())),
        );
        // Seed the SHARED slot the Disk panel renders (SystemProjectionStore, latest-wins
        // from `directory_usage_events`) — the same field a live platform
        // batch fills through the shell fold.
        seed_projection_fact(
            &mut app.shell,
            ProjectionSeedFact::DirectoryUsage(Some(demo_directory_usage())),
        );
        seed_demo_npu_inventory(&mut app);
        seed_demo_history(&mut app);
        // The demo boot has no persisted appearance preference, so the shared
        // `TM_SKIN` testing override is the only appearance input here. When it
        // is unset/invalid the `ThemeParams::default()` (GNOME dark) set by
        // `from_shell` is preserved verbatim; production never reads it.
        if let Some(forced) = crate::theme::forced_theme_params_from_env() {
            app.theme_params = forced;
        }
        capture_overrides::apply_capture_overrides(&mut app);
        app
    }
}

/// Total demo history depth (in frames) seeded for the CPU/Memory/disk/network
/// main charts. The demo runtime performs no live collection, so without this
/// seed every main graph would sit on the honest-but-useless "Collecting
/// samples…" cold-start placeholder forever (the shell demo frame records
/// exactly one sample). 36 frames fill one visible 60-sample window with the
/// same smooth measured-looking shape the GPU evidence scene uses.
pub(crate) const DEMO_HISTORY_FRAMES: usize = 36;

pub fn demo_app() -> TuiApp {
    TuiApp::demo()
}
