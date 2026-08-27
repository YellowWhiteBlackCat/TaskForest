//! Tests for graph settings, language preference, history seeding, and modal animation transitions.

use std::time::Duration;

use super::*;
use crate::app::SettingsChange;
use crate::test_support::temp_dir;
use taskmanager_application::{AppPage, ConfigStore};
use taskmanager_shell::ShellKeyEvent;
use taskmanager_theme::tokens::MotionPolicy;

#[test]
fn pristine_first_launch_applies_defaults_without_a_recovery_notice() {
    let dir = temp_dir("settings-pristine-default");
    let path = dir.join("config.json");
    let app = IcedApp::with_config_store(None, ConfigStore::new(&path));

    assert_eq!(
        app.config_draft(),
        taskmanager_application::Config::default()
    );
    assert!(app.shell.feedback_notice().is_none());

    drop(app);
    let _ = std::fs::remove_dir_all(dir);
}

/// The details modal always opens on the Overview tab (GPUI parity), resets
/// when the properties overlay transitions open, and the tab pills route
/// through the frontend-local section selector.
#[test]
fn details_section_resets_to_overview_when_properties_open() {
    let mut app = IcedApp::demo();
    assert_eq!(app.details_section(), DetailsSection::Overview);

    // Opening the properties overlay through the real keyboard route
    // (Enter, Applications page) must land on Overview even after a previous
    // visit ended on another tab.
    app.shell.application.active_page = AppPage::Applications;
    assert!(app.shell.select_row(0));
    let enter = IcedKey::Fixed(ShellKeyEvent::new(
        taskmanager_application::KeyCode::Enter,
        taskmanager_application::Modifiers::NONE,
    ));
    let _ = app.update(Message::Key(enter));
    assert!(app.process_properties_open());
    assert_eq!(app.details_section(), DetailsSection::Overview);

    let _ = app.update(Message::SelectDetailsSection(DetailsSection::Insights));
    assert_eq!(app.details_section(), DetailsSection::Insights);

    // Close and reopen: the tab returns to Overview.
    let _ = app.update(Message::DismissOverlay);
    let _ = app.update(Message::Key(enter));
    assert!(app.process_properties_open());
    assert_eq!(app.details_section(), DetailsSection::Overview);

    // Every tab has a stable focus operation id.
    for section in DetailsSection::ALL {
        assert_eq!(
            crate::focus::focus_id(FocusTarget::DetailsTab(section)),
            format!("iced-details-tab-{}", section.key())
        );
    }
}

/// The Insights escalation pill routes through the shared one-shot
/// escalation effect: queueing it against a demo (no platform) reports the
/// demo suppression honestly instead of panicking.
#[test]
fn network_escalation_request_queues_through_the_shared_effect() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::RequestProcessNetworkEscalation);
    assert!(app.shell.feedback_text().contains("Demo mode"));
}

/// The persisted graph-data-points preference reaches the SHARED history
/// store at both config edges (G-02): the settings change resizes it live,
/// and a fresh launch applies it from disk through `load_config`.
#[test]
fn graph_data_points_preference_propagates_to_the_shared_history_capacity() {
    let dir = temp_dir("settings-history-capacity");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    // `with_config_store` applies the product config before returning, so the
    // loaded config default—not the shell primitive's pre-config capacity—is
    // authoritative here.
    assert_eq!(
        app.shell.history.capacity(),
        usize::try_from(taskmanager_application::Config::default().graph_data_points).unwrap()
    );

    let _ = app.update(Message::SettingsChanged(SettingsChange::GraphDataPoints(
        240,
    )));
    assert_eq!(app.shell.history.capacity(), 240, "live settings edge");
    app.wait_for_config_where(|config| config.graph_data_points == 240);

    // A fresh launch restores the persisted window from disk.
    let mut reloaded = IcedApp::with_config_store(None, ConfigStore::new(&path));
    reloaded.load_config();
    assert_eq!(
        reloaded.shell.history.capacity(),
        240,
        "startup config edge"
    );
    // The clamp keeps a corrupted preference inside the product bounds.
    let hostile_store = ConfigStore::new(&path);
    let mut config = hostile_store.load_or_default();
    config.graph_data_points = u32::MAX;
    hostile_store.save(&config).unwrap();
    let mut hostile = IcedApp::with_config_store(None, ConfigStore::new(&path));
    hostile.load_config();
    assert_eq!(
        hostile.shell.history.capacity(),
        taskmanager_shell::history::MAX_HISTORY_CAPACITY,
        "a hostile preference clamps to the product ceiling"
    );

    drop(hostile);
    drop(reloaded);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The language preference round-trips through the shared `Config::language`
/// token (G-22): the settings picker writes "en"/"zh", a fresh launch applies
/// it and pins the shared catalog before the first frame.
#[test]
fn language_preference_persists_and_applies_at_startup() {
    let dir = temp_dir("settings-language");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(app.language(), crate::i18n::Language::En);

    let _ = app.update(Message::SettingsChanged(SettingsChange::Language(
        crate::i18n::Language::Zh,
    )));
    app.wait_for_config_where(|config| config.language.as_deref() == Some("zh"));
    let config = ConfigStore::new(&path).load_or_default();
    assert_eq!(
        config.language.as_deref(),
        Some("zh"),
        "token write-through"
    );
    assert_eq!(app.language(), crate::i18n::Language::Zh);

    // A fresh launch applies the persisted token and pins the shared catalog.
    let mut reloaded = IcedApp::with_config_store(None, ConfigStore::new(&path));
    reloaded.load_config();
    assert_eq!(reloaded.language(), crate::i18n::Language::Zh);
    assert_eq!(
        taskmanager_application::i18n::current_language(),
        taskmanager_application::i18n::Language::Zh,
        "the shared catalog follows the applied preference"
    );

    // An unknown persisted token keeps the frontend default (first-launch
    // sentinel semantics); the explicit "en" token restores English.
    let hostile_store = ConfigStore::new(&path);
    let mut config = hostile_store.load_or_default();
    config.language = Some("klingon".into());
    hostile_store.save(&config).unwrap();
    let mut hostile = IcedApp::with_config_store(None, ConfigStore::new(&path));
    hostile.load_config();
    assert_eq!(
        hostile.language(),
        crate::i18n::Language::En,
        "an unknown token keeps the default"
    );

    drop(hostile);
    drop(reloaded);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The motion preference round-trips through the shared `Config::motion`
/// token (G-23) and seeds the frontend's process-wide policy at both config
/// edges: the Settings segmented control writes the token and installs the
/// policy live, a fresh launch installs the persisted policy from disk, and
/// a hostile token degrades to the full-motion default without a panic.
// test-intent: behavior
#[test]
fn motion_preference_seeds_the_policy_and_round_trips() {
    let dir = temp_dir("settings-motion");
    let path = dir.join("config.json");
    let mut app = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(app.motion_policy(), MotionPolicy::Normal);
    assert_eq!(app.preferences().motion, "normal");

    let _ = app.update(Message::SettingsChanged(SettingsChange::Motion(
        MotionPolicy::Reduced,
    )));
    assert_eq!(
        app.motion_policy(),
        MotionPolicy::Reduced,
        "the live settings edge installs the policy"
    );
    assert_eq!(app.preferences().motion, "reduced");
    app.wait_for_config_where(|config| config.motion == "reduced");

    // A fresh launch installs the persisted policy from disk.
    let reloaded = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(
        reloaded.motion_policy(),
        MotionPolicy::Reduced,
        "the startup config edge installs the persisted policy"
    );
    assert_eq!(reloaded.preferences().motion, "reduced");
    drop(reloaded);

    // A hostile persisted token degrades to the full-motion default — never
    // a panic and never a fabricated stronger restriction.
    let hostile_store = ConfigStore::new(&path);
    let mut config = hostile_store.load_or_default();
    config.motion = "warp-speed".into();
    hostile_store.save(&config).unwrap();
    let hostile = IcedApp::with_config_store(None, ConfigStore::new(&path));
    assert_eq!(
        hostile.motion_policy(),
        MotionPolicy::Normal,
        "an unknown token degrades to Normal"
    );

    // The token↔policy mapping is total in both directions over the shared
    // vocabulary (case/whitespace tolerant on the read side).
    for (policy, token) in [
        (MotionPolicy::Normal, "normal"),
        (MotionPolicy::Reduced, "reduced"),
        (MotionPolicy::NoMotion, "none"),
    ] {
        assert_eq!(crate::app::motion::motion_token(policy), token);
        assert_eq!(crate::app::motion::motion_policy_from_token(token), policy);
    }
    assert_eq!(
        crate::app::motion::motion_policy_from_token(""),
        MotionPolicy::Normal
    );
    assert_eq!(
        crate::app::motion::motion_policy_from_token("  No-Motion "),
        MotionPolicy::NoMotion
    );

    drop(hostile);
    drop(app);
    std::fs::remove_dir_all(dir).unwrap();
}

/// The persisted NoMotion preference reaches the modal-entrance path through
/// the installed policy: opening a modal paints the final state directly
/// (progress starts at 1.0), so the per-frame pump never engages for the
/// entrance — the same open path that sweeps 0→1 under the default policy.
// test-intent: behavior
#[test]
fn no_motion_preference_drops_the_modal_entrance_to_the_final_state() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SettingsChanged(SettingsChange::Motion(
        MotionPolicy::NoMotion,
    )));
    assert_eq!(app.preferences().motion, "none");

    let _ = app.update(Message::OpenSettings);
    let appear = app.input.modal_appear.as_ref().expect("entrance recorded");
    assert_eq!(
        appear.progress(),
        1.0,
        "no-motion entrance starts at the final state"
    );
    assert!(
        !app.frame_pump_active(),
        "a finished entrance never drives the frame pump"
    );
}

#[test]
fn rejected_config_submission_rolls_renderer_preferences_back_to_canonical() {
    let dir = temp_dir("settings-backpressure-rollback");
    let path = dir.join("config.json");
    std::fs::create_dir_all(&dir).unwrap();
    let coordinator = taskmanager_application::ConfigCoordinator::start_with_options(
        ConfigStore::new(&path),
        taskmanager_application::ConfigRuntimeOptions {
            command_capacity: 1,
            publication_capacity: 4,
            refresh_interval: std::time::Duration::from_secs(60),
        },
    )
    .unwrap();
    let mut app = IcedApp::new_with_runtime_clients(None, Some(coordinator.client()), None);
    app.load_config();
    app.shell.application.active_page = taskmanager_application::AppPage::Applications;
    app.input.focused_control = Some(FocusTarget::PageTab(
        taskmanager_application::AppPage::Applications,
    ));
    let lock = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path.with_extension("json.lock"))
        .unwrap();
    lock.try_lock().unwrap();

    let base = app
        .configuration
        .client()
        .and_then(taskmanager_application::ConfigClient::snapshot)
        .unwrap()
        .as_ref()
        .clone();
    let mut blocked = base.clone();
    blocked.ui_size = "Small".into();
    assert_eq!(
        app.configuration.client().unwrap().try_submit(blocked),
        Ok(taskmanager_application::ConfigSubmissionStatus::Queued)
    );
    // Once the worker has taken the first command it blocks on the fixture's
    // OS lock. Queue a second command into the now-free one-slot lane; from
    // that accepted state the Settings submission below is deterministically
    // rejected without a wall-clock assumption.
    let mut queued_behind_blocked = false;
    for _ in 0..10_000 {
        let mut queued = base.clone();
        queued.ui_size = "Large".into();
        match app.configuration.client().unwrap().try_submit(queued) {
            Ok(taskmanager_application::ConfigSubmissionStatus::Queued) => {
                queued_behind_blocked = true;
                break;
            }
            Err(taskmanager_application::ConfigSubmitError::Backpressure) => {
                std::thread::yield_now();
            }
            outcome => panic!("unexpected queue outcome: {outcome:?}"),
        }
    }
    assert!(queued_behind_blocked, "fixture must fill the bounded lane");

    let _ = app.update(Message::SettingsChanged(SettingsChange::Skin(Skin::Kde)));
    assert_eq!(app.theme().skin, Skin::Gnome);
    assert!(app.preferences().skin.is_empty());
    assert_eq!(
        app.shell.page(),
        taskmanager_application::AppPage::Applications
    );
    assert_eq!(
        app.input.focused_control,
        Some(FocusTarget::PageTab(
            taskmanager_application::AppPage::Applications
        ))
    );
    assert!(app.shell.feedback_text().contains("not queued"));

    drop(lock);
    drop(app);
    drop(coordinator);
    std::fs::remove_dir_all(dir).unwrap();
}

/// Opening the details overlay on a process carrying provider-pre-populated
/// history renders that history immediately (G-14): the per-process ring is
/// seeded from `ProcessItem.*_history` at the open transition, before any
/// live tick sampled it. A process without provider history (mac/win shape)
/// keeps the honest empty ring.
#[test]
fn overlay_open_seeds_the_process_ring_from_provider_history() {
    let mut app = IcedApp::demo();
    let _ = app.update(Message::SelectPage(AppPage::Applications));
    let mut seeded = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(3_100)
        .name("provider-fed".into())
        .current_cpu_percentage(44.0)
        .current_memory_bytes(2_048)
        .current_start_time_secs(1_785_290_000)
        .cpu_history(vec![10.0, 20.0, 30.0])
        .mem_history(vec![1_000.0, 1_500.0])
        .disk_read_history(vec![5.0, 6.0, 7.0])
        .disk_write_history(vec![])
        .build();
    // The overlay path freezes a trustworthy identity (mirrors the shared
    // demo shell's process shape).
    let mut seeded_observations = *seeded.scalar_observations();
    seeded_observations.start_token =
        taskmanager_application::ScalarObservation::available(310_001, 1);
    seeded.apply_scalar_observations(seeded_observations);
    let mut cold = taskmanager_test_support::ProcessItemFixtureBuilder::new()
        .pid(3_101)
        .name("mac-win-shape".into())
        .current_cpu_percentage(1.0)
        .current_memory_bytes(512)
        .current_start_time_secs(1_785_290_001)
        .build();
    let mut cold_observations = *cold.scalar_observations();
    cold_observations.start_token =
        taskmanager_application::ScalarObservation::available(310_002, 1);
    cold.apply_scalar_observations(cold_observations);
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Processes(Some(vec![seeded, cold])),
    );

    // Select the provider-fed process and open the properties overlay through
    // the real keyboard route (Enter) — the open transition seeds the ring.
    app.shell.selected = 0;
    app.shell.query = String::new();
    let visible_pids: Vec<u32> = app
        .shell
        .visible_processes()
        .iter()
        .map(|process| process.pid)
        .collect();
    let index = visible_pids
        .iter()
        .position(|pid| *pid == 3_100)
        .expect("fixture row visible");
    let _ = app.shell.select_row(index);
    let enter = || {
        Message::Key(IcedKey::Fixed(ShellKeyEvent::new(
            taskmanager_application::KeyCode::Enter,
            taskmanager_application::Modifiers::NONE,
        )))
    };
    let _ = app.update(enter());
    assert!(app.process_properties_open());

    let ring = app
        .process_perf_history()
        .expect("the overlay open must seed the ring");
    assert_eq!(ring.pid(), 3_100);
    assert_eq!(ring.cpu_samples(), vec![10.0, 20.0, 30.0]);
    assert_eq!(ring.memory_samples(), vec![1_000.0, 1_500.0]);
    assert_eq!(ring.disk_read_samples(), vec![5.0, 6.0, 7.0]);
    assert!(
        ring.disk_write_samples().is_empty(),
        "an empty provider window seeds nothing — never a fabricated series"
    );

    // Re-open on the mac/win-shaped process: the seed is a no-op and the ring
    // re-points with an honest empty window (the live-sampling fallback).
    let _ = app.update(Message::DismissOverlay);
    let index = visible_pids
        .iter()
        .position(|pid| *pid == 3_101)
        .expect("cold fixture row visible");
    let _ = app.shell.select_row(index);
    let _ = app.update(enter());
    let ring = app
        .process_perf_history()
        .expect("the re-open keeps a ring for live sampling");
    assert_eq!(ring.pid(), 3_101);
    assert!(ring.is_empty(), "no provider history → no fabricated seed");
}

/// The modal-entrance progress sweeps 0→1 over the appear-token duration on
/// iced's own easing engine (`EaseOutCubic`): monotone, decelerating, and
/// clamped. Pure — the tick/frame pump advances it, the renderer only reads.
#[test]
fn modal_appear_advances_eased_purely_and_clamps() {
    let now = std::time::Instant::now();
    let appear = ModalAppear::new(MotionPolicy::Normal, now);
    assert_eq!(appear.progress(), 0.0);
    let quarter = appear.clone().advance(now + Duration::from_millis(45));
    let half = appear.clone().advance(now + Duration::from_millis(90));
    // 90 ms of the 180 ms token is the linear midpoint; EaseOutCubic passes
    // it faster (1 - 0.5³ = 0.875) — the decelerating fade the GPUI modal
    // uses, now from iced's animation engine instead of a linear ramp.
    assert!(
        (half.progress() - 0.875).abs() < 1e-3,
        "ease-out midpoint: got {}",
        half.progress()
    );
    assert!(
        quarter.progress() > 0.0 && quarter.progress() < half.progress(),
        "monotone deceleration: quarter={} half={}",
        quarter.progress(),
        half.progress()
    );
    let done = appear.advance(now + Duration::from_secs(5));
    assert!((done.progress() - 1.0).abs() < 1e-6, "clamped at 1.0");
}

/// The entrance honors the shared motion policy: `Reduced` caps the sweep at
/// the fast token (80 ms) and `NoMotion` paints the final state with no
/// sweep at all — the same contract the GPUI motion helpers enforce.
#[test]
fn modal_appear_honors_motion_policy() {
    let now = std::time::Instant::now();
    let reduced = ModalAppear::new(MotionPolicy::Reduced, now);
    assert!(
        (reduced.advance(now + Duration::from_millis(80)).progress() - 1.0).abs() < 1e-6,
        "reduced completes within the 80 ms fast token"
    );
    let frozen = ModalAppear::new(MotionPolicy::NoMotion, now);
    assert_eq!(
        frozen.advance(now).progress(),
        1.0,
        "no-motion starts at the final state (nothing to pump)"
    );
}

/// Opening a modal starts the entrance; closing clears it; the tick advances
/// it; the view reads the progress (never a clock).
#[test]
fn modal_appear_starts_on_open_advances_on_tick_and_clears_on_close() {
    let mut app = IcedApp::demo();
    assert!(app.input.modal_appear.is_none());
    assert_eq!(
        app.modal_appear_progress(),
        1.0,
        "no modal -> fully visible"
    );

    // Open settings: the entrance starts at 0.
    let _ = app.update(Message::OpenSettings);
    let appear = app.input.modal_appear.as_ref().expect("entrance started");
    assert_eq!(appear.progress(), 0.0);

    // A tick advances it (the ramp math itself is covered by the pure
    // advance test; here the tick keeps the state alive and bounded).
    let _ = app.update(Message::Tick);
    let appear = app.input.modal_appear.as_ref().expect("entrance continues");
    assert!(appear.progress() >= 0.0 && appear.progress() <= 1.0);

    // Close: the state clears and the progress reads 1.0 again.
    let _ = app.update(Message::CloseSettings);
    assert!(app.input.modal_appear.is_none());
    assert_eq!(app.modal_appear_progress(), 1.0);
}

/// The warm-up spinner revolves only while the shell is still collecting its
/// first frame, wraps its phase within one revolution, and never starts
/// under the no-motion policy; the per-frame pump follows exactly the states
/// that animate.
#[test]
fn warmup_spin_and_frame_pump_lifecycle() {
    let now = std::time::Instant::now();
    let spin = WarmupSpin::new(MotionPolicy::Normal, now).expect("spins under normal policy");
    assert_eq!(spin.phase(), 0.0);
    let quarter = spin.advance(now + Duration::from_millis(200));
    let half = spin.advance(now + Duration::from_millis(400));
    assert!(
        (quarter.phase() - 0.25).abs() < 1e-3 && (half.phase() - 0.5).abs() < 1e-3,
        "phase tracks the 800 ms period"
    );
    let wrapped = spin.advance(now + Duration::from_millis(900));
    assert!(
        (wrapped.phase() - 0.125).abs() < 1e-3,
        "phase wraps within one revolution instead of clamping"
    );
    assert!(
        WarmupSpin::new(MotionPolicy::NoMotion, now).is_none(),
        "no-motion renders the static arc"
    );

    // Lifecycle: the demo fixture ships a committed frame (Ready — nothing
    // spins, the pump idles). Dropping the committed snapshot models the
    // real launch window before the first telemetry frame: the spinner
    // starts, the pump engages, the phase advances per frame, and the first
    // committed frame stops both again.
    let mut app = IcedApp::demo();
    assert!(
        !app.shell.telemetry_frame_state().is_collecting(),
        "demo ships a committed frame"
    );
    app.advance_motion(now);
    assert_eq!(app.warmup_spin_phase(), None, "ready shell never spins");
    assert!(
        !app.frame_pump_active(),
        "nothing animating -> the pump idles between events"
    );

    let committed = app.shell.projection().snapshot.clone();
    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(None)),
    );
    assert!(committed.is_some(), "fixture snapshot present");
    assert!(
        app.shell.telemetry_frame_state().is_collecting(),
        "no committed frame -> collecting"
    );
    app.advance_motion(now);
    assert_eq!(
        app.warmup_spin_phase(),
        Some(0.0),
        "collecting shell spins from phase 0"
    );
    assert!(app.frame_pump_active(), "the spinner drives the frame pump");
    app.advance_motion(now + Duration::from_millis(100));
    assert!(
        app.warmup_spin_phase()
            .is_some_and(|phase| (phase - 0.125).abs() < 1e-3),
        "phase advanced by the 800 ms period fraction"
    );

    taskmanager_shell::fixture::seed_projection_fact(
        &mut app.shell,
        taskmanager_shell::fixture::ProjectionSeedFact::Snapshot(Box::new(committed)),
    );
    app.advance_motion(now + Duration::from_millis(200));
    assert_eq!(
        app.warmup_spin_phase(),
        None,
        "the first committed frame stops the spinner"
    );
    assert!(
        !app.frame_pump_active(),
        "pump disengages once nothing animates"
    );
}

/// The progress-aware modal styles keep the token derivation (alpha ramps,
/// never literals): the scrim darkening scales with the progress and the
/// panel background fades in.
#[test]
fn modal_styles_scale_with_the_entrance_progress() {
    let theme = taskmanager_theme::Theme::dark();
    let dim = crate::theme::scrim_style_with(&theme, 0.0);
    let full = crate::theme::scrim_style_with(&theme, 1.0);
    let brightness = |style: &iced::widget::container::Style| match style.background.unwrap() {
        iced::Background::Color(color) => (color.r + color.g + color.b) as f64,
        other => panic!("unexpected background {other:?}"),
    };
    assert!(
        brightness(&full) < brightness(&dim),
        "the scrim darkens as the progress reaches 1.0"
    );
    assert_eq!(brightness(&full), brightness(&full), "deterministic");

    let panel_dim = crate::theme::elevated_style_with(&theme, 0.0);
    let panel_full = crate::theme::elevated_style_with(&theme, 1.0);
    let panel_dim_alpha = match panel_dim.background.unwrap() {
        iced::Background::Color(color) => color.a,
        other => panic!("unexpected background {other:?}"),
    };
    let panel_full_alpha = match panel_full.background.unwrap() {
        iced::Background::Color(color) => color.a,
        other => panic!("unexpected background {other:?}"),
    };
    assert!(panel_dim_alpha < panel_full_alpha, "panel fades in");
    assert_eq!(panel_full_alpha, 1.0, "fully visible at progress 1.0");
}
