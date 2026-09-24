//! test-intent: behavior
//!
//! The real-time thermal-status / PROCHOT delivery on the Bevy Performance
//! CPU diagnostic strip: the mounted row renders the shared
//! [`msr_thermal_status_summary`] fold beside the cumulative counters and
//! keeps the shared dash while the privileged `telemetry.cpu.msr` lane
//! produced no accepted readout.
//!
//! Split out of `performance.rs` so the line-budget gate keeps both files
//! under the test hard limit.

use bevy::app::App;
use taskmanager_application::i18n::Language;
use taskmanager_application::i18n::set_language;
use taskmanager_application::i18n::t;
use taskmanager_application::{
    CorrelatedEvent, MsrReadoutEvent, PlatformEventBatch, PlatformEventContext,
};
use taskmanager_core::core::metrics::{MsrReadoutSnapshot, MsrThermalStatusReadout};
use taskmanager_platform_contract::{CapabilityId, EventSequence, RequestId};
use taskmanager_shell::presentation::MISSING_VALUE;
use taskmanager_shell::presentation::msr_thermal_status_summary;

use super::tests::{dyn_text_value, headless_perf_app, route_to_performance};
use super::{CpuField, DynField};
use crate::app::FrontendTrack;
use crate::drain::ShellProjectionFolded;

/// The real-time thermal-status / PROCHOT delivery: the Performance CPU
/// diagnostic strip renders the shared
/// [`msr_thermal_status_summary`] fold beside the cumulative counters — the
/// shared asserted/clear words and the honest dash for an unreadable register.
/// The mounted row keeps the shared dash while the privileged
/// `telemetry.cpu.msr` lane produced no accepted readout.
#[test]
fn cpu_real_time_thermal_status_renders_with_honest_absence() {
    set_language(Language::En);
    let field = DynField::Cpu(CpuField::ThermalStatus);
    let mut app = headless_perf_app();
    app.update();
    route_to_performance(&mut app);
    assert_eq!(
        dyn_text_value(app.world_mut(), &field).as_deref(),
        Some(MISSING_VALUE),
        "the mounted real-time row must keep the shared dash until the lane runs"
    );

    set_msr_thermal_status(&mut app, &[(0, Some(true)), (1, Some(false)), (2, None)]);
    let expected = msr_thermal_status_summary(
        app.world_mut()
            .non_send_mut::<FrontendTrack>()
            .shell
            .msr_readout_state(),
    )
    .expect("an accepted readout must fold");
    assert_eq!(
        dyn_text_value(app.world_mut(), &field).as_deref(),
        Some(expected.as_str()),
        "the folded readout must reach the mounted DynText row as the shared fold"
    );
    assert!(
        expected.contains(t("cpu.thermal_status_asserted"))
            && expected.contains(t("cpu.thermal_status_clear")),
        "the fold must carry the shared asserted/clear vocabulary: {expected}"
    );
    assert!(
        expected.contains(MISSING_VALUE),
        "an unreadable register must keep the honest dash: {expected}"
    );
}

/// Seed the shared MSR session with an accepted readout carrying the given
/// real-time `IA32_THERM_STATUS` bits, then fire the drain trigger so the
/// mounted row is rewritten from the folded projection.
fn set_msr_thermal_status(app: &mut App, bits: &[(u32, Option<bool>)]) {
    let mut batch = PlatformEventBatch::default();
    {
        let shell = &mut app.world_mut().non_send_mut::<FrontendTrack>().shell;
        let attempt = shell.begin_msr_readout_request();
        let request_id = RequestId::new(41).expect("fixture request id");
        assert!(shell.accept_msr_readout_request(attempt, request_id));
        batch.msr_readout_events.push(CorrelatedEvent::new(
            PlatformEventContext {
                request_id,
                capability: CapabilityId::TELEMETRY_CPU_MSR,
                provider: None,
                sequence: EventSequence::new(1),
                observed_at_ms: 10,
            },
            MsrReadoutEvent::Update(
                MsrReadoutSnapshot::success(Vec::new()).with_thermal(
                    bits.iter()
                        .map(|&(cpu, thermal_status)| MsrThermalStatusReadout {
                            cpu,
                            thermal_status,
                            ..MsrThermalStatusReadout::default()
                        })
                        .collect(),
                ),
            ),
        ));
        shell.apply_platform_batch(batch);
    }
    app.world_mut().commands().trigger(ShellProjectionFolded);
    app.update();
    app.update();
}
