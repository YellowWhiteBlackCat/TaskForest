//! Storage providers: filesystem health, the SMART self-test lanes, and the
//! directory-usage scan lane.

use super::*;

impl DirectoryUsageProvider for FakeProvider {
    fn scan_chunk(
        &mut self,
        spec: &DirectoryScanSpec,
        control: &DirectoryScanControl,
        observed_at_ms: u64,
    ) -> Result<DirectoryUsageSnapshot, ProviderFailure> {
        // One bounded call that terminates immediately: the contract fixture
        // only needs the directory-usage lane wired so the capability is
        // published; it does not exercise real scan semantics.
        Ok(DirectoryUsageSnapshot {
            scan_id: control.scan_id(),
            root: spec.root.clone(),
            status: DirectoryScanStatus::Completed,
            entries: Vec::new(),
            totals: DirectoryScanTotals::fresh(observed_at_ms),
        })
    }
}

impl FilesystemHealthProvider for FakeProvider {
    fn refresh(
        &mut self,
        _observed_at_ms: u64,
    ) -> Result<CompositeSourceSnapshot<FilesystemHealthSnapshot>, ProviderFailure> {
        thread::sleep(self.delay);
        Ok(CompositeSourceSnapshot::new(
            FilesystemHealthSnapshot::default(),
            vec![fixture_source(
                "fixture.filesystem",
                0,
                self.observation_source_failure,
            )],
        ))
    }
}

impl SmartSelfTestControlProvider for FakeProvider {
    fn start(
        &mut self,
        intent: &SmartSelfTestIntent,
        observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        thread::sleep(self.smart_control_delay);
        if let Ok(mut starts) = self.smart_starts.lock() {
            starts.push(intent.clone());
        }
        if let Some(report) = &self.smart_control_report {
            return Ok(report.clone());
        }
        Ok(SmartSelfTestReport {
            state: DeviceState::healthy(observed_at_ms),
            phase: taskmanager_core::SmartSelfTestPhase::Running,
            kind: Some(intent.kind),
            ..SmartSelfTestReport::default()
        })
    }
}

impl SmartSelfTestObservationProvider for FakeProvider {
    fn refresh(
        &mut self,
        target: &StorageDeviceTarget,
        previous: DeviceState,
        _observed_at_ms: u64,
    ) -> Result<SmartSelfTestReport, ProviderFailure> {
        self.smart_refresh_started.store(true, Ordering::Release);
        if let Ok(mut targets) = self.smart_refresh_targets.lock() {
            targets.push(target.clone());
        }
        thread::sleep(self.smart_refresh_delay);
        if let Ok(errors) = self.smart_refresh_errors.lock()
            && let Some((_, failure)) = errors
                .iter()
                .find(|(locator, _)| locator == target.locator.as_str())
        {
            return Err(*failure);
        }
        if let Ok(reports) = self.smart_refresh_reports.lock()
            && let Some((_, report)) = reports
                .iter()
                .find(|(locator, _)| locator == target.locator.as_str())
        {
            return Ok(report.clone());
        }
        Ok(SmartSelfTestReport {
            state: previous,
            ..SmartSelfTestReport::default()
        })
    }
}
