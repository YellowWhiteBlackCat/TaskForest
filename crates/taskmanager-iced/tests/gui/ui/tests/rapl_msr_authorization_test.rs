//! Behavior tests for RAPL package power and MSR readouts authorization in Iced.

use taskmanager_application::{
    CorrelatedEvent, MsrReadoutEvent, MsrReadoutRequest, PlatformEffect, PlatformEventBatch,
    PlatformEventContext, RaplPowerEvent, RaplPowerRequest,
};
use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::metrics::{
    MsrPackageReadout, MsrReadoutSnapshot, RaplPackageRow, RaplPowerSnapshot,
};
use taskmanager_platform_contract::{CapabilityId, CapabilityStatus, EventSequence, RequestId};

use crate::app::{FocusTarget, Message, PerfDevice};
use crate::focus::focus_id;
use crate::ui::perf_overview::{
    cpu_memory_header_and_stats, msr_readout_needs_authorization, msr_readouts_card,
    rapl_power_card, rapl_power_needs_authorization,
};

#[test]
fn test_authorize_rapl_power_message_yields_effect() {
    let mut app = crate::IcedApp::demo();

    // 1. Direct handle_control_message
    let effect = app.handle_control_message(Message::AuthorizeRaplPower);
    assert_eq!(
        effect,
        Some(PlatformEffect::RaplPower(RaplPowerRequest::Refresh))
    );

    // 2. Through reducer dispatch
    let dispatch = app.reduce_control_message(Message::AuthorizeRaplPower);
    assert_eq!(
        dispatch.effect,
        Some(PlatformEffect::RaplPower(RaplPowerRequest::Refresh))
    );
}

#[test]
fn test_authorize_msr_readouts_message_yields_effect() {
    let mut app = crate::IcedApp::demo();

    // 1. Direct handle_control_message
    let effect = app.handle_control_message(Message::AuthorizeMsrReadouts);
    assert_eq!(
        effect,
        Some(PlatformEffect::MsrReadout(MsrReadoutRequest::Refresh))
    );

    // 2. Through reducer dispatch
    let dispatch = app.reduce_control_message(Message::AuthorizeMsrReadouts);
    assert_eq!(
        dispatch.effect,
        Some(PlatformEffect::MsrReadout(MsrReadoutRequest::Refresh))
    );
}

#[test]
fn test_focus_targets_for_rapl_and_msr_authorization() {
    assert_eq!(
        focus_id(FocusTarget::AuthorizeRaplPower),
        "iced-authorize-rapl-power"
    );
    assert_eq!(
        focus_id(FocusTarget::AuthorizeMsrReadouts),
        "iced-authorize-msr-readouts"
    );

    assert!(FocusTarget::ALL.contains(&FocusTarget::AuthorizeRaplPower));
    assert!(FocusTarget::ALL.contains(&FocusTarget::AuthorizeMsrReadouts));

    let app = crate::IcedApp::demo();
    assert_eq!(
        app.focus_request_for(&Message::AuthorizeRaplPower),
        Some(FocusTarget::AuthorizeRaplPower)
    );
    assert_eq!(
        app.focus_request_for(&Message::AuthorizeMsrReadouts),
        Some(FocusTarget::AuthorizeMsrReadouts)
    );
}

#[test]
fn test_rapl_power_authorization_and_ready_presentation() {
    taskmanager_test_support::pin_english();
    let mut app = crate::IcedApp::demo();

    // 1. When escalation is required via rejection
    let attempt = app.shell.begin_rapl_power_request();
    assert!(
        app.shell
            .reject_rapl_power_request(attempt, FailureKind::RequiresEscalation)
    );

    assert!(rapl_power_needs_authorization(
        app.shell.rapl_power_state(),
        None,
    ));

    assert!(
        rapl_power_card(&app, app.theme()).is_some(),
        "RAPL card renders authorization button when escalation is required"
    );

    // 2. When authorization is needed via capability status while Closed
    app.shell.close_rapl_power_request();
    assert!(rapl_power_needs_authorization(
        app.shell.rapl_power_state(),
        Some(CapabilityStatus::Available),
    ));
    assert!(rapl_power_needs_authorization(
        app.shell.rapl_power_state(),
        Some(CapabilityStatus::PermissionRequired),
    ));

    // 3. When ready with observations
    let attempt = app.shell.begin_rapl_power_request();
    let request_id = RequestId::new(10).expect("fixture request id");
    assert!(app.shell.accept_rapl_power_request(attempt, request_id));

    let mut batch = PlatformEventBatch::default();
    batch.rapl_power_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::TELEMETRY_CPU_PACKAGE_POWER,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 10,
        },
        RaplPowerEvent::Update(RaplPowerSnapshot::success(
            200,
            vec![RaplPackageRow {
                name: "Package 0".to_string(),
                power_w: 45.2,
                energy_delta_uj: 1_000_000,
            }],
        )),
    ));
    app.shell.apply_platform_batch(batch);

    assert!(!rapl_power_needs_authorization(
        app.shell.rapl_power_state(),
        None,
    ));

    assert!(
        rapl_power_card(&app, app.theme()).is_some(),
        "RAPL card renders observations when ready"
    );

    let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
    let power_stat = stats
        .iter()
        .find(|s| s.label().contains("Package power") || s.label() == "Package 0");
    assert!(
        power_stat.is_some(),
        "CPU stat rows include package power when ready"
    );
    assert_eq!(power_stat.unwrap().value(), Some("45.2 W"));
}

#[test]
fn test_msr_readouts_authorization_and_ready_presentation() {
    taskmanager_test_support::pin_english();
    let mut app = crate::IcedApp::demo();

    // 1. When escalation is required via rejection
    let attempt = app.shell.begin_msr_readout_request();
    assert!(
        app.shell
            .reject_msr_readout_request(attempt, FailureKind::RequiresEscalation)
    );

    assert!(msr_readout_needs_authorization(
        app.shell.msr_readout_state(),
        None,
    ));

    assert!(
        msr_readouts_card(&app, app.theme()).is_some(),
        "MSR card renders authorization button when escalation is required"
    );

    // 2. When authorization is needed via capability status while Closed
    app.shell.close_msr_readout_request();
    assert!(msr_readout_needs_authorization(
        app.shell.msr_readout_state(),
        Some(CapabilityStatus::Available),
    ));
    assert!(msr_readout_needs_authorization(
        app.shell.msr_readout_state(),
        Some(CapabilityStatus::PermissionRequired),
    ));

    // 3. When ready with observations
    let attempt = app.shell.begin_msr_readout_request();
    let request_id = RequestId::new(20).expect("fixture request id");
    assert!(app.shell.accept_msr_readout_request(attempt, request_id));

    let mut batch = PlatformEventBatch::default();
    batch.msr_readout_events.push(CorrelatedEvent::new(
        PlatformEventContext {
            request_id,
            capability: CapabilityId::TELEMETRY_CPU_MSR,
            provider: None,
            sequence: EventSequence::new(1),
            observed_at_ms: 10,
        },
        MsrReadoutEvent::Update(MsrReadoutSnapshot::success(vec![MsrPackageReadout {
            cpu: 0,
            bclk_mhz: None,
            temperature_c: Some(68.5),
            multiplier: Some(45.0),
            multiplier_min: Some(8.0),
            multiplier_max: Some(58.0),
            vcore_v: Some(1.25),
        }])),
    ));
    app.shell.apply_platform_batch(batch);

    assert!(!msr_readout_needs_authorization(
        app.shell.msr_readout_state(),
        None,
    ));

    assert!(
        msr_readouts_card(&app, app.theme()).is_some(),
        "MSR card renders observations when ready"
    );

    let (_, _, stats) = cpu_memory_header_and_stats(&app, PerfDevice::Cpu);
    let temp_stat = stats
        .iter()
        .find(|s| s.label().contains("CPU 0") && s.label().contains("Temperature"));
    assert!(
        temp_stat.is_some(),
        "CPU stat rows include MSR temperature when ready"
    );
    assert_eq!(temp_stat.unwrap().value(), Some("68.5 °C"));

    let mult_stat = stats
        .iter()
        .find(|s| s.label().contains("CPU 0") && s.label().contains("Multiplier"));
    assert!(
        mult_stat.is_some(),
        "CPU stat rows include MSR multiplier when ready"
    );
    assert_eq!(mult_stat.unwrap().value(), Some("×45.0"));
}
