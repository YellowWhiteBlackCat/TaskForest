//! Service providers: inventory, dependencies, control, log snapshot, and log
//! stream.

use super::*;

impl ServiceInventoryProvider for FakeProvider {
    fn refresh(&mut self) -> Result<PartialSourceSnapshot<ServiceItem>, ProviderFailure> {
        thread::sleep(self.delay);
        if let Some(error) = self.service_error {
            return Err(error);
        }
        Ok(PartialSourceSnapshot::new(
            vec![ServiceItem::default()],
            vec![fixture_source(
                "fixture.service.selected",
                1,
                self.observation_source_failure,
            )],
        ))
    }
}

impl ServiceDependenciesProvider for FakeProvider {
    fn dependencies(&mut self, service_id: &ServiceId) -> Result<ServiceDeps, ProviderFailure> {
        if let Some(error) = self.service_operation_error {
            return Err(error);
        }
        let mut dependencies = ServiceDeps::default();
        dependencies.replace_relation_targets(
            ServiceRelationKind::Requires,
            [ServiceId::new(format!("{service_id}.socket"))],
        );
        Ok(dependencies)
    }
}

impl ServiceControlProvider for FakeProvider {
    fn control(
        &mut self,
        _service_id: &ServiceId,
        _action: ServiceAction,
    ) -> Result<(), ProviderFailure> {
        if let Some(error) = self.service_operation_error {
            return Err(error);
        }
        Ok(())
    }
}

impl ServiceLogSnapshotProvider for FakeProvider {
    fn snapshot(&mut self, service_id: &ServiceId) -> Result<ServiceLogState, ProviderFailure> {
        if let Some(error) = self.service_operation_error {
            return Err(error);
        }
        Ok(ServiceLogState::from_lines(vec![format!(
            "{service_id}: ready"
        )]))
    }
}

impl ServiceLogStreamProvider for FakeProvider {
    fn stream(
        &mut self,
        _query: &ServiceLogQuery,
        _observed_at_ms: u64,
    ) -> Result<ServiceLogStreamState, ProviderFailure> {
        if let Some(error) = self.service_operation_error {
            return Err(error);
        }
        Ok(ServiceLogStreamState::Empty)
    }
}
