use super::*;
use taskmanager_shell::fixture::health::health_fixture;

#[test]
fn fixture_exposes_healthy_read_only_error_and_missing_sensor_branches() {
    let fixture = health_fixture();
    assert_eq!(fixture.filesystems.filesystems.len(), 3);
    assert!(
        fixture
            .filesystems
            .filesystems
            .iter()
            .any(|filesystem| { filesystem.status == FilesystemHealthStatus::ReadOnly })
    );
    assert!(
        fixture
            .filesystems
            .filesystems
            .iter()
            .any(|filesystem| { filesystem.status == FilesystemHealthStatus::ErrorsReported })
    );
    assert!(
        fixture
            .sensors
            .readings
            .iter()
            .any(|reading| reading.current_measurement().is_none())
    );
}
