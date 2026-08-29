//! Process providers: list, insight lanes (network / GPU / resources /
//! isolation), affinity, and control.

use super::*;
use taskmanager_core::ProcessResourceObservations;
use taskmanager_core::core::process_telemetry::{ProcessEnvironment, ProcessEnvironmentEntry};
use taskmanager_platform_provider::ProcessEnvironmentProvider;

impl ProcessListProvider for FakeProvider {
    fn refresh(
        &mut self,
        _observed_at_ms: u64,
    ) -> Result<PartialSourceSnapshot<ProcessItem>, ProviderFailure> {
        self.process_refresh_started.store(true, Ordering::Release);
        thread::sleep(self.delay);
        let items = vec![
            taskmanager_test_support::ProcessItemFixtureBuilder::new()
                .pid(42)
                .name("worker".into())
                .build(),
        ];
        Ok(PartialSourceSnapshot::new(
            items,
            vec![fixture_source(
                "fixture.process.list",
                1,
                self.observation_source_failure,
            )],
        ))
    }
}

impl ProcessNetworkProvider for FakeProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessNetworkSnapshot>, ProviderFailure> {
        thread::sleep(self.process_telemetry_delay);
        if let Ok(mut targets) = self.process_telemetry_targets.lock() {
            targets.push(target.clone());
        }
        let healthy = DeviceState::healthy(observed_at_ms);
        Ok(ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 7_500,
            },
            value: ProcessNetworkSnapshot {
                state: healthy,
                traffic_state: healthy,
                ..ProcessNetworkSnapshot::default()
            },
        })
    }
}

impl ProcessGpuProvider for FakeProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessGpuSnapshot>, ProviderFailure> {
        thread::sleep(self.process_gpu_delay);
        let state = self.process_telemetry_failure.map_or_else(
            || DeviceState::healthy(observed_at_ms),
            |failure| DeviceState {
                status: DeviceStatus::from_failure(failure),
                last_success_ms: None,
            },
        );
        Ok(ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 7_500,
            },
            value: ProcessGpuSnapshot {
                state,
                ..ProcessGpuSnapshot::default()
            },
        })
    }
}

impl ProcessResourcesProvider for FakeProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessResourceSnapshot>, ProviderFailure> {
        Ok(ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 7_500,
            },
            value: ProcessResourceSnapshot::from_observations(
                DeviceState::healthy(observed_at_ms),
                ProcessResourceObservations::default(),
                Vec::new(),
            ),
        })
    }
}

impl ProcessIsolationProvider for FakeProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessIsolation>, ProviderFailure> {
        Ok(ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 7_500,
            },
            value: ProcessIsolation {
                state: DeviceState::healthy(observed_at_ms),
                ..ProcessIsolation::default()
            },
        })
    }
}

impl ProcessThreadsProvider for FakeProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessThreads>, ProviderFailure> {
        Ok(ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 7_500,
            },
            value: ProcessThreads {
                state: DeviceState::healthy(observed_at_ms),
                threads: Vec::new(),
            },
        })
    }
}

impl ProcessOpenFilesProvider for FakeProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessOpenFiles>, ProviderFailure> {
        Ok(ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 7_500,
            },
            value: ProcessOpenFiles {
                state: DeviceState::healthy(observed_at_ms),
                ..ProcessOpenFiles::default()
            },
        })
    }
}

impl ProcessEnvironmentProvider for FakeProvider {
    fn observe(
        &mut self,
        target: &FrozenProcessIdentity,
        observed_at_ms: u64,
    ) -> Result<ProcessInsightSnapshot<ProcessEnvironment>, ProviderFailure> {
        Ok(ProcessInsightSnapshot {
            identity: ProcessIdentity {
                pid: target.pid,
                start_token: 7_500,
            },
            value: ProcessEnvironment {
                state: DeviceState::healthy(observed_at_ms),
                working_directory: Some(std::path::PathBuf::from("/srv/app")),
                entries: vec![ProcessEnvironmentEntry {
                    key: "FIXTURE".into(),
                    value: "1".into(),
                }],
                truncated_count: 0,
            },
        })
    }
}

impl ProcessAffinityProvider for FakeProvider {
    fn affinity(&mut self, _target: &FrozenProcessIdentity) -> Result<Vec<u32>, ProviderFailure> {
        Ok(vec![0, 1])
    }
}

impl ProcessAffinityControlProvider for FakeProvider {
    fn set_affinity(
        &mut self,
        _target: &FrozenProcessIdentity,
        _cpus: &[u32],
    ) -> Result<(), ProviderFailure> {
        Ok(())
    }
}

impl ProcessResourceControlProvider for FakeProvider {
    fn apply_limits(
        &mut self,
        _target: &FrozenProcessIdentity,
        _limits: &ResourceGroupLimitRequest,
    ) -> Result<(), ProviderFailure> {
        Ok(())
    }
}

impl ProcessNetworkEscalationProvider for FakeProvider {
    fn request_capture_escalation(&mut self) -> Result<(), ProviderFailure> {
        Ok(())
    }
}

impl ProcessControlProvider for FakeProvider {
    fn end_task(&mut self, target: FrozenProcessIdentity) -> Result<(), ProviderFailure> {
        if let Ok(mut ended) = self.ended.lock() {
            ended.push(target);
        }
        Ok(())
    }

    fn execute_batch(
        &mut self,
        intent: taskmanager_core::ProcessBatchIntent,
    ) -> Result<taskmanager_core::ProcessBatchResult, ProviderFailure> {
        self.process_control_started.store(true, Ordering::Release);
        thread::sleep(self.process_control_delay);
        let targets = intent
            .targets
            .iter()
            .cloned()
            .map(|target| (target, taskmanager_core::ProcessBatchTargetResult::Applied))
            .collect();
        Ok(taskmanager_core::ProcessBatchResult { intent, targets })
    }

    fn send_signal(
        &mut self,
        target: &FrozenProcessIdentity,
        signal: taskmanager_core::ProcessSignal,
    ) -> Result<(), ProviderFailure> {
        if let Ok(mut signaled) = self.signaled.lock() {
            signaled.push((target.pid, signal));
        }
        Ok(())
    }
}
