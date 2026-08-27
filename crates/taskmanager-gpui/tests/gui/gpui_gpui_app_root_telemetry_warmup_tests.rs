use super::{TelemetryWarmupPhase, telemetry_warmup_phase};
use std::time::Duration;

#[test]
fn warmup_watchdog_progresses_without_fabricating_provider_failure() {
    assert_eq!(
        telemetry_warmup_phase(Duration::from_secs(0)),
        TelemetryWarmupPhase::Collecting
    );
    assert_eq!(
        telemetry_warmup_phase(Duration::from_secs(5)),
        TelemetryWarmupPhase::Slow
    );
    assert_eq!(
        telemetry_warmup_phase(Duration::from_secs(15)),
        TelemetryWarmupPhase::Retryable
    );
    assert!(TelemetryWarmupPhase::Retryable.allows_retry());
}
