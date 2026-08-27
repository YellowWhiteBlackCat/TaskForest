//! Environment providers: startup inventory / evidence / control and session
//! inventory / control.

use super::*;

impl StartupInventoryProvider for FakeProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<StartupEntry>, ProviderFailure> {
        Ok(PartialSourceSnapshot::new(
            Vec::new(),
            vec![fixture_source(
                "fixture.startup",
                0,
                self.observation_source_failure,
            )],
        ))
    }
}

impl StartupEvidenceProvider for FakeProvider {
    fn observe(
        &mut self,
        observed_at_ms: u64,
    ) -> Result<StartupBootEvidenceSnapshot, ProviderFailure> {
        self.startup_evidence_times
            .lock()
            .expect("startup evidence times")
            .push(observed_at_ms);
        Ok(StartupBootEvidenceSnapshot::default())
    }
}

impl StartupControlProvider for FakeProvider {
    fn set_enabled(&mut self, entry: &StartupEntry, enabled: bool) -> Result<(), ProviderFailure> {
        thread::sleep(self.delay);
        if let Ok(mut controls) = self.startup_controls.lock() {
            controls.push((entry.name.clone(), enabled));
        }
        Ok(())
    }
}

impl SessionInventoryProvider for FakeProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<SessionItem>, ProviderFailure> {
        Ok(PartialSourceSnapshot::new(
            Vec::new(),
            vec![fixture_source(
                "fixture.sessions",
                0,
                self.observation_source_failure,
            )],
        ))
    }
}

impl SessionControlProvider for FakeProvider {
    fn control(
        &mut self,
        session_id: &SessionId,
        action: SessionControlAction,
    ) -> Result<(), ProviderFailure> {
        thread::sleep(self.delay);
        if let Ok(mut controls) = self.session_controls.lock() {
            controls.push((session_id.to_string(), action));
        }
        Err(ProviderFailure::PermissionDenied)
    }
}
