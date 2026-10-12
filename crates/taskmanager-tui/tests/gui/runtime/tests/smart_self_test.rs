//! TUI-013: the `t` chord on the Performance·Disk device arms the shared
//! SMART self-test confirmation gate. Every downstream step reuses the
//! existing typed seams: the gate's `y` emits `PlatformEffect::SmartControl`,
//! the runtime queues it through `queue_effect` (which runs the shell's
//! request session: begin → submit → accept/reject), and `n`/Esc dismiss
//! without any platform work.

use super::super::*;

use ratatui::Terminal;
use ratatui::backend::TestBackend;
use taskmanager_application::i18n::{Language, set_language};
use taskmanager_application::{
    AppAction, AppPage, SmartControlRequest, SmartObservationBatch, SmartSelfTestState,
};
use taskmanager_core::core::identity::{DeviceGeneration, DeviceId};
use taskmanager_core::core::metrics::SmartAvailability;
use taskmanager_core::core::smart::{
    SmartSelfTestCapabilities, SmartSelfTestKind, SmartSelfTestPhase, SmartSelfTestReport,
    SmartSelfTestSupport,
};
use taskmanager_core::core::system_health::SmartSelfTestObservation;
use taskmanager_core::core::target::StorageDeviceKey;

use crate::{TuiApp, TuiTheme, render};
use taskmanager_application::{PendingConfirmation, PlatformClient};
use taskmanager_shell::fixture::edit_snapshot;
use taskmanager_shell::fixture::health::seed_smart_observation_batch;
use taskmanager_shell::queue_effect;

/// Seed explicit device support on the synthetic disk through the shell's
/// typed snapshot seam. SMART health availability is independent.
fn seed_smart_capable_disk(app: &mut TuiApp) {
    edit_snapshot(&mut app.shell, |snapshot| {
        let snapshot = snapshot.as_mut().expect("demo snapshot");
        for disk in &mut snapshot.disks {
            disk.smart_availability = SmartAvailability::Available;
            disk.smart_self_test_capabilities = SmartSelfTestCapabilities {
                short: SmartSelfTestSupport::Supported,
                extended: SmartSelfTestSupport::Unsupported,
                conveyance: SmartSelfTestSupport::Unknown,
            };
        }
    });
}

fn on_disk_device(app: &mut TuiApp) {
    let _ = app.apply_action(AppAction::SelectPage(AppPage::Performance));
    app.perf_device = crate::PerfDevice::Disk;
}

fn press_t(
    app: &mut TuiApp,
    modifiers: ratatui::crossterm::event::KeyModifiers,
) -> Option<PlatformEffect> {
    handle_key(
        app,
        KeyEvent::new(ratatui::crossterm::event::KeyCode::Char('t'), modifiers),
    )
}

fn press(app: &mut TuiApp, code: ratatui::crossterm::event::KeyCode) -> Option<PlatformEffect> {
    handle_key(
        app,
        KeyEvent::new(code, ratatui::crossterm::event::KeyModifiers::NONE),
    )
}

fn armed_app() -> TuiApp {
    let mut app = crate::demo_app();
    on_disk_device(&mut app);
    seed_smart_capable_disk(&mut app);
    let _ = press_t(&mut app, ratatui::crossterm::event::KeyModifiers::NONE);
    app
}

fn seed_smart_reports(
    app: &mut TuiApp,
    reports: Vec<(DeviceId, DeviceGeneration, SmartSelfTestPhase)>,
) {
    let revision = app
        .projection()
        .smart_projection()
        .0
        .revision()
        .checked_next()
        .expect("fixture revision advances");
    seed_smart_observation_batch(
        &mut app.shell,
        SmartObservationBatch {
            revision,
            subject: None,
            observations: reports
                .into_iter()
                .map(
                    |(device_id, device_generation, phase)| SmartSelfTestObservation {
                        device_id,
                        device_generation,
                        device_key: StorageDeviceKey::new("fixture-device"),
                        display_name: String::from("Fixture disk"),
                        report: SmartSelfTestReport {
                            phase,
                            ..SmartSelfTestReport::default()
                        },
                    },
                )
                .collect(),
            issues: Vec::new(),
            ended: Vec::new(),
        },
    );
}

// ── the arm ───────────────────────────────────────────────────────────────

/// The chord arms the shared gate and freezes the demo disk's full identity:
/// provider locator = device name, model preferred as display name,
/// generation bound. Arming is local state only — no platform work.
#[test]
fn t_on_a_smart_capable_disk_arms_the_shared_confirmation_with_the_frozen_identity() {
    let mut app = crate::demo_app();
    on_disk_device(&mut app);
    seed_smart_capable_disk(&mut app);

    let effect = press_t(&mut app, ratatui::crossterm::event::KeyModifiers::NONE);
    assert!(
        effect.is_none(),
        "arming opens only the shared gate, no platform work"
    );
    let Some(PendingConfirmation::SmartSelfTest(intent)) = app.pending_confirmation().cloned()
    else {
        panic!("the declared t chord must arm the SMART self-test gate");
    };
    assert_eq!(intent.kind, SmartSelfTestKind::Short);
    assert_eq!(intent.device_id.as_str(), "disk:demo:nvme0");
    assert_eq!(intent.device_generation.get(), 1);
    assert_eq!(intent.device_key.as_str(), "nvme0n1");
    assert_eq!(
        intent.display_name, "TiPro9000 2TB",
        "the model is the display name, exactly what the dialog shows"
    );
}

/// The armed gate renders the confirmation dialog naming kind and disk — the
/// same dialog vocabulary the shell gate's `y` then submits.
#[test]
fn armed_gate_renders_the_named_self_test_dialog() {
    let app = armed_app();
    let _guard = crate::ui::test_support::LANG_TEST_GUARD
        .lock()
        .expect("lang test guard");
    set_language(Language::En);
    let backend = TestBackend::new(100, 30);
    let mut terminal = Terminal::new(backend).expect("test terminal");
    terminal
        .draw(|frame| render(frame, &app, TuiTheme::default()))
        .expect("draw");
    let text = terminal.backend().to_string();
    assert!(text.contains("SMART self-test"), "dialog title: {text}");
    assert!(
        text.contains("Short"),
        "the dialog names the test kind: {text}"
    );
    assert!(
        text.contains("TiPro9000 2TB"),
        "the dialog names the frozen disk: {text}"
    );
}

// ── the guards ────────────────────────────────────────────────────────────

/// Without a SMART-capable disk, on another Performance device, on another
/// page, or under Ctrl — the chord stays honestly inert.
#[test]
fn t_stays_inert_without_a_smart_capable_target_or_off_its_scope() {
    // Healthy SMART data alone is not evidence of self-test support.
    let mut app = crate::demo_app();
    on_disk_device(&mut app);
    edit_snapshot(&mut app.shell, |snapshot| {
        for disk in &mut snapshot.as_mut().expect("demo snapshot").disks {
            disk.smart_availability = SmartAvailability::Available;
            disk.smart_self_test_capabilities = SmartSelfTestCapabilities::default();
        }
    });
    let effect = press_t(&mut app, ratatui::crossterm::event::KeyModifiers::NONE);
    assert!(effect.is_none());
    assert!(
        app.pending_confirmation().is_none(),
        "no SMART support, no gate"
    );

    // SMART-capable, but the wrong Performance device.
    let mut gpu = crate::demo_app();
    let _ = gpu.apply_action(AppAction::SelectPage(AppPage::Performance));
    gpu.perf_device = crate::PerfDevice::Gpu;
    seed_smart_capable_disk(&mut gpu);
    let effect = press_t(&mut gpu, ratatui::crossterm::event::KeyModifiers::NONE);
    assert!(effect.is_none());
    assert!(
        gpu.pending_confirmation().is_none(),
        "the arm is Disk-scoped"
    );

    // SMART-capable, but a different page entirely.
    let mut services = crate::demo_app();
    let _ = services.apply_action(AppAction::SelectPage(AppPage::Services));
    seed_smart_capable_disk(&mut services);
    let effect = press_t(&mut services, ratatui::crossterm::event::KeyModifiers::NONE);
    assert!(effect.is_none());
    assert!(
        services.pending_confirmation().is_none(),
        "the arm is Performance-scoped"
    );

    // Chorded variants stay unwired, like every page command.
    let mut chorded = crate::demo_app();
    on_disk_device(&mut chorded);
    seed_smart_capable_disk(&mut chorded);
    let effect = press_t(
        &mut chorded,
        ratatui::crossterm::event::KeyModifiers::CONTROL,
    );
    assert!(effect.is_none());
    assert!(
        chorded.pending_confirmation().is_none(),
        "Ctrl refuses the chord"
    );
}

#[test]
fn running_exact_target_blocks_both_keyboard_and_palette_arming() {
    use crate::PaletteLocalAction;

    let mut keyboard = crate::demo_app();
    on_disk_device(&mut keyboard);
    seed_smart_capable_disk(&mut keyboard);
    let target = crate::menus::smart_self_test_target(&keyboard).expect("capable target");
    seed_smart_reports(
        &mut keyboard,
        vec![(
            target.device_id.clone(),
            target.device_generation,
            SmartSelfTestPhase::Running,
        )],
    );

    let effect = press_t(&mut keyboard, ratatui::crossterm::event::KeyModifiers::NONE);
    assert!(
        effect.is_none(),
        "a Running target must not produce an effect"
    );
    assert!(
        keyboard.pending_confirmation().is_none(),
        "the direct chord must not arm a gate for an exact Running report"
    );

    let mut palette = crate::demo_app();
    on_disk_device(&mut palette);
    seed_smart_capable_disk(&mut palette);
    let target = crate::menus::smart_self_test_target(&palette).expect("capable target");
    seed_smart_reports(
        &mut palette,
        vec![(
            target.device_id,
            target.device_generation,
            SmartSelfTestPhase::Running,
        )],
    );
    palette.run_palette_local_action(Some(PaletteLocalAction::RequestSmartSelfTest));
    assert!(
        palette.pending_confirmation().is_none(),
        "the palette twin must use the same exact-target Running guard"
    );
}

// ── the shared gate ───────────────────────────────────────────────────────

/// `y` on the armed gate emits the typed `SmartControl(StartSelfTest)` effect
/// carrying exactly the identity the dialog displayed, and the gate closes.
#[test]
fn y_confirms_the_displayed_intent_into_the_typed_smart_effect() {
    let mut app = armed_app();
    assert!(app.pending_confirmation().is_some(), "precondition: armed");

    let effect = press(&mut app, ratatui::crossterm::event::KeyCode::Char('y'));
    let Some(PlatformEffect::SmartControl(SmartControlRequest::StartSelfTest(intent))) = effect
    else {
        panic!("confirm must emit the typed SmartControl effect, got {effect:?}");
    };
    assert_eq!(intent.device_id.as_str(), "disk:demo:nvme0");
    assert_eq!(intent.device_generation.get(), 1);
    assert_eq!(intent.device_key.as_str(), "nvme0n1");
    assert_eq!(intent.kind, SmartSelfTestKind::Short);
    assert!(
        app.pending_confirmation().is_none(),
        "the gate consumed the confirmation"
    );
}

#[test]
fn y_dismisses_when_the_frozen_target_becomes_running_after_arm() {
    let mut app = armed_app();
    let Some(PendingConfirmation::SmartSelfTest(intent)) = app.pending_confirmation().cloned()
    else {
        panic!("precondition: the SMART confirmation is armed");
    };
    seed_smart_reports(
        &mut app,
        vec![(
            intent.device_id,
            intent.device_generation,
            SmartSelfTestPhase::Running,
        )],
    );

    let effect = press(&mut app, ratatui::crossterm::event::KeyCode::Char('y'));
    assert!(effect.is_none(), "Running recheck must suppress the effect");
    assert!(
        app.pending_confirmation().is_none(),
        "the stale confirmation gate must close"
    );
    assert_eq!(
        app.shell.smart_self_test_state(),
        &SmartSelfTestState::Idle,
        "a rejected confirmation must not start a request session"
    );
}

#[test]
fn running_reports_for_other_device_lifecycles_do_not_block_confirmation() {
    let mut app = armed_app();
    let Some(PendingConfirmation::SmartSelfTest(intent)) = app.pending_confirmation().cloned()
    else {
        panic!("precondition: the SMART confirmation is armed");
    };
    let other_generation = DeviceGeneration::new(
        intent
            .device_generation
            .get()
            .checked_add(1)
            .expect("fixture generation advances"),
    );
    seed_smart_reports(
        &mut app,
        vec![
            (
                DeviceId::new("disk:other"),
                intent.device_generation,
                SmartSelfTestPhase::Running,
            ),
            (
                intent.device_id.clone(),
                other_generation,
                SmartSelfTestPhase::Running,
            ),
        ],
    );

    let effect = press(&mut app, ratatui::crossterm::event::KeyCode::Char('y'));
    let Some(PlatformEffect::SmartControl(SmartControlRequest::StartSelfTest(confirmed))) = effect
    else {
        panic!("only the exact device lifecycle can block the frozen intent");
    };
    assert_eq!(confirmed, intent);
}

/// `n` and Esc dismiss without platform work and leave the request session
/// untouched at Idle — a cancelled request never looks started.
#[test]
fn n_and_esc_dismiss_without_platform_work_and_leave_the_session_idle() {
    let mut app = armed_app();
    let effect = press(&mut app, ratatui::crossterm::event::KeyCode::Char('n'));
    assert!(effect.is_none(), "dismissal must not produce an effect");
    assert!(app.pending_confirmation().is_none());
    assert_eq!(
        app.shell.smart_self_test_state(),
        &SmartSelfTestState::Idle,
        "a cancelled request never opened a session"
    );

    // Re-arm, then Esc: the shared-surface dismissal path stays equally clean.
    let _ = press_t(&mut app, ratatui::crossterm::event::KeyModifiers::NONE);
    assert!(app.pending_confirmation().is_some());
    let effect = press(&mut app, ratatui::crossterm::event::KeyCode::Esc);
    assert!(effect.is_none());
    assert!(app.pending_confirmation().is_none());
    assert_eq!(app.shell.smart_self_test_state(), &SmartSelfTestState::Idle);
}

/// While the gate is armed it owns the keyboard: `t` again is swallowed as a
/// gate character, so the chord cannot silently re-freeze a new target.
#[test]
fn while_armed_the_gate_owns_the_keyboard() {
    let mut app = armed_app();
    let _ = press_t(&mut app, ratatui::crossterm::event::KeyModifiers::NONE);
    assert!(
        matches!(
            app.pending_confirmation(),
            Some(PendingConfirmation::SmartSelfTest(_))
        ),
        "the gate swallows non-gate characters"
    );
}

// ── the request session round-trip ────────────────────────────────────────

/// The full round-trip through the shared seam: `t` → gate → `y` →
/// `SmartControl` effect → `queue_effect` → the provider port receives the
/// typed `StartSelfTest`, and the accepted submission opens the shell's
/// Loading session naming the same frozen intent — the honest "requested"
/// state until the provider's correlated terminal arrives.
#[test]
fn confirm_round_trips_through_queue_effect_into_the_smart_control_session() {
    use std::sync::{Arc, Mutex};
    use taskmanager_application::{
        PlatformEvent, PlatformFacets, PlatformHandle, SmartControlRequest, StorageFacets,
    };
    use taskmanager_platform_contract::{
        CapabilityCatalog, CapabilitySnapshot, EventEnvelope, EventPort, EventPortError,
        RequestEnvelope, RequestPort, SubmissionError,
    };

    #[derive(Default)]
    struct EmptyCapabilities;
    impl CapabilityCatalog for EmptyCapabilities {
        fn snapshot(&self) -> CapabilitySnapshot {
            CapabilitySnapshot::default()
        }
    }

    #[derive(Default)]
    struct EmptyEvents;
    impl EventPort for EmptyEvents {
        type Event = PlatformEvent;

        fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
            Ok(None)
        }
    }

    #[derive(Default)]
    struct RecordingSmartControl(Mutex<Vec<SmartControlRequest>>);
    impl RequestPort for RecordingSmartControl {
        type Request = SmartControlRequest;

        fn try_submit(
            &self,
            request: RequestEnvelope<Self::Request>,
        ) -> Result<(), SubmissionError> {
            self.0
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .push(request.payload);
            Ok(())
        }
    }

    let recorded = Arc::new(RecordingSmartControl::default());
    let mut client = PlatformClient::new(PlatformHandle::new(
        Arc::new(EmptyCapabilities),
        Arc::new(EmptyEvents),
        PlatformFacets::default()
            .with_storage(StorageFacets::default().with_smart_control(recorded.clone())),
    ));

    let mut app = armed_app();
    let effect = press(&mut app, ratatui::crossterm::event::KeyCode::Char('y'))
        .expect("confirm yields the typed effect");

    // The runtime queues every key effect through the shared seam.
    queue_effect(&mut app.shell, &mut client, effect);

    let submitted = recorded
        .0
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    assert_eq!(submitted.len(), 1, "exactly one SMART request is submitted");
    let SmartControlRequest::StartSelfTest(intent) = &submitted[0] else {
        panic!(
            "the provider must receive StartSelfTest: {:?}",
            submitted[0]
        );
    };
    assert_eq!(intent.device_id.as_str(), "disk:demo:nvme0");
    assert_eq!(intent.device_generation.get(), 1);
    match app.shell.smart_self_test_state() {
        SmartSelfTestState::Loading(loading) => {
            assert_eq!(
                loading.intent.device_id.as_str(),
                "disk:demo:nvme0",
                "the session names the same frozen intent"
            );
        }
        other => panic!("the accepted request must sit in the Loading session: {other:?}"),
    }
}

// ── the palette lane ──────────────────────────────────────────────────────

/// The palette row executes the same arm under the same scope: it refuses a
/// wrong device and arms the identical gate from the Disk device.
#[test]
fn palette_runs_the_same_smart_arm_under_the_same_scope() {
    use crate::PaletteLocalAction;

    let mut cpu = crate::demo_app();
    let _ = cpu.apply_action(AppAction::SelectPage(AppPage::Performance));
    cpu.perf_device = crate::PerfDevice::Cpu;
    seed_smart_capable_disk(&mut cpu);
    cpu.run_palette_local_action(Some(PaletteLocalAction::RequestSmartSelfTest));
    assert!(
        cpu.pending_confirmation().is_none(),
        "the palette arm is Disk-scoped like the chord"
    );

    let mut disk = crate::demo_app();
    on_disk_device(&mut disk);
    seed_smart_capable_disk(&mut disk);
    disk.run_palette_local_action(Some(PaletteLocalAction::RequestSmartSelfTest));
    assert!(matches!(
        disk.pending_confirmation(),
        Some(PendingConfirmation::SmartSelfTest(_))
    ));
}
