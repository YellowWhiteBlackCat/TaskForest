//! System-axis request submission on `PlatformClient`: six-domain telemetry
//! under one revision plus hardware-inventory refresh.

use taskmanager_platform_contract::{CapabilityId, RequestId, SubmissionError};

use crate::platform::{
    ContainerRollupRequest, CpuTelemetryRequest, GpuEngineRowsRequest, GpuTelemetryRequest,
    HardwareInventoryRequest, HostTelemetryRequest, MemoryTelemetryRequest,
    NetworkTelemetryRequest, NpuInventoryRequest, StorageTelemetryRequest, SystemTelemetryDomain,
    SystemTelemetrySubmission, SystemTelemetrySubmissionError, SystemTelemetryUnavailable,
};

use super::{PendingSystemTelemetryRequest, PlatformClient, submit_request};

impl PlatformClient {
    /// Schedule all six domains under one application-owned revision.
    ///
    /// Every bounded port returns its own outcome; already accepted work is
    /// never rolled back or hidden behind an aggregate result.
    pub fn submit_system_telemetry(
        &mut self,
        submitted_at_ms: u64,
    ) -> Result<SystemTelemetrySubmission, SystemTelemetrySubmissionError> {
        let revision = self.begin_system_telemetry_revision(&SystemTelemetryDomain::ALL)?;
        let host = self.submit_host_telemetry(revision, submitted_at_ms);
        let cpu = self.submit_cpu_telemetry(revision, submitted_at_ms);
        let memory = self.submit_memory_telemetry(revision, submitted_at_ms);
        let storage = self.submit_storage_telemetry(revision, submitted_at_ms);
        let network = self.submit_network_telemetry(revision, submitted_at_ms);
        let gpu = self.submit_gpu_telemetry(revision, submitted_at_ms);
        let projection = self
            .system_telemetry_projection
            .snapshot()
            .unwrap_or_else(|| crate::ProjectedSystemTelemetry::pending(revision));
        Ok(SystemTelemetrySubmission {
            revision,
            host,
            cpu,
            memory,
            storage,
            network,
            gpu,
            projection,
        })
    }

    pub(super) fn system_refresh_results(
        &mut self,
        submitted_at_ms: u64,
    ) -> Vec<Result<RequestId, SubmissionError>> {
        match self.submit_system_telemetry(submitted_at_ms) {
            Ok(submission) => submission.into_request_results(),
            Err(SystemTelemetrySubmissionError::RevisionExhausted) => SystemTelemetryDomain::ALL
                .into_iter()
                .map(SystemTelemetryDomain::capability)
                .map(|capability| {
                    Err(SubmissionError {
                        capability,
                        kind: taskmanager_platform_contract::SubmissionErrorKind::InvalidRequest,
                    })
                })
                .collect(),
        }
    }

    pub(super) fn scheduled_system_refresh_results(
        &mut self,
        due_capabilities: &[CapabilityId],
        submitted_at_ms: u64,
    ) -> Vec<(CapabilityId, Result<RequestId, SubmissionError>)> {
        let domains: Vec<_> = SystemTelemetryDomain::ALL
            .into_iter()
            .filter(|domain| due_capabilities.contains(&domain.capability()))
            .collect();
        if domains.is_empty() {
            return Vec::new();
        }
        let revision = match self.begin_system_telemetry_revision(&domains) {
            Ok(revision) => revision,
            Err(SystemTelemetrySubmissionError::RevisionExhausted) => {
                return domains
                    .into_iter()
                    .map(|domain| {
                        let capability = domain.capability();
                        (
                            capability.clone(),
                            Err(SubmissionError {
                                capability,
                                kind: taskmanager_platform_contract::SubmissionErrorKind::InvalidRequest,
                            }),
                        )
                    })
                    .collect();
            }
        };
        domains
            .into_iter()
            .map(|domain| {
                (
                    domain.capability(),
                    self.submit_system_telemetry_domain(domain, revision, submitted_at_ms),
                )
            })
            .collect()
    }

    fn submit_system_telemetry_domain(
        &mut self,
        domain: SystemTelemetryDomain,
        revision: crate::SystemTelemetryRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        match domain {
            SystemTelemetryDomain::Host => self.submit_host_telemetry(revision, submitted_at_ms),
            SystemTelemetryDomain::Cpu => self.submit_cpu_telemetry(revision, submitted_at_ms),
            SystemTelemetryDomain::Memory => {
                self.submit_memory_telemetry(revision, submitted_at_ms)
            }
            SystemTelemetryDomain::Storage => {
                self.submit_storage_telemetry(revision, submitted_at_ms)
            }
            SystemTelemetryDomain::Network => {
                self.submit_network_telemetry(revision, submitted_at_ms)
            }
            SystemTelemetryDomain::Gpu => self.submit_gpu_telemetry(revision, submitted_at_ms),
        }
    }

    fn submit_host_telemetry(
        &mut self,
        revision: crate::SystemTelemetryRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().system().host(),
            submitted_at_ms,
            HostTelemetryRequest { revision },
        );
        self.finish_system_telemetry_submission(id, revision, SystemTelemetryDomain::Host, result)
    }

    fn submit_cpu_telemetry(
        &mut self,
        revision: crate::SystemTelemetryRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().system().cpu(),
            submitted_at_ms,
            CpuTelemetryRequest { revision },
        );
        self.finish_system_telemetry_submission(id, revision, SystemTelemetryDomain::Cpu, result)
    }

    fn submit_memory_telemetry(
        &mut self,
        revision: crate::SystemTelemetryRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().system().memory(),
            submitted_at_ms,
            MemoryTelemetryRequest { revision },
        );
        self.finish_system_telemetry_submission(id, revision, SystemTelemetryDomain::Memory, result)
    }

    fn submit_storage_telemetry(
        &mut self,
        revision: crate::SystemTelemetryRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().system().storage(),
            submitted_at_ms,
            StorageTelemetryRequest { revision },
        );
        self.finish_system_telemetry_submission(
            id,
            revision,
            SystemTelemetryDomain::Storage,
            result,
        )
    }

    fn submit_network_telemetry(
        &mut self,
        revision: crate::SystemTelemetryRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().system().network(),
            submitted_at_ms,
            NetworkTelemetryRequest { revision },
        );
        self.finish_system_telemetry_submission(
            id,
            revision,
            SystemTelemetryDomain::Network,
            result,
        )
    }

    fn submit_gpu_telemetry(
        &mut self,
        revision: crate::SystemTelemetryRevision,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        let result = submit_request(
            id,
            self.handle.facets().system().gpu(),
            submitted_at_ms,
            GpuTelemetryRequest { revision },
        );
        self.finish_system_telemetry_submission(id, revision, SystemTelemetryDomain::Gpu, result)
    }

    fn finish_system_telemetry_submission(
        &mut self,
        id: RequestId,
        revision: crate::SystemTelemetryRevision,
        domain: SystemTelemetryDomain,
        result: Result<(), SubmissionError>,
    ) -> Result<RequestId, SubmissionError> {
        match result {
            Ok(()) => {
                // One accepted request per system domain is sufficient for
                // correlation. Production ECS already enforces this; retaining
                // by domain also bounds test/embedded ports that do not use the
                // native runtime.
                self.system_telemetry_requests
                    .retain(|_, pending| pending.domain != domain);
                self.system_telemetry_requests
                    .insert(id, PendingSystemTelemetryRequest { revision, domain });
                Ok(id)
            }
            Err(error) => {
                let _ = self.system_telemetry_projection.apply_failure(
                    revision,
                    domain,
                    SystemTelemetryUnavailable::Submission(error.kind),
                );
                Err(error)
            }
        }
    }

    fn next_system_telemetry_revision(
        &mut self,
    ) -> Result<crate::SystemTelemetryRevision, SystemTelemetrySubmissionError> {
        let next = self
            .system_telemetry_revision
            .checked_next()
            .ok_or(SystemTelemetrySubmissionError::RevisionExhausted)?;
        self.system_telemetry_revision = next;
        Ok(next)
    }

    fn begin_system_telemetry_revision(
        &mut self,
        domains: &[SystemTelemetryDomain],
    ) -> Result<crate::SystemTelemetryRevision, SystemTelemetrySubmissionError> {
        let revision = self.next_system_telemetry_revision()?;
        self.system_telemetry_projection
            .begin_domains(revision, domains);
        Ok(revision)
    }

    pub fn submit_hardware_inventory(
        &mut self,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().system().hardware_inventory(),
            submitted_at_ms,
            HardwareInventoryRequest::Refresh,
        )?;
        Ok(id)
    }

    pub fn submit_container_rollup(
        &mut self,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().system().containers(),
            submitted_at_ms,
            ContainerRollupRequest::Refresh,
        )?;
        Ok(id)
    }

    /// Submit a per-engine GPU utilization read for one device (capability
    /// `telemetry.gpu.engines`). The provider performs ONE bounded PMU helper
    /// invocation per request; rows or a typed failure arrive as
    /// `GpuEngineRowsEvent` publications in the next event batch.
    pub fn submit_gpu_engine_rows(
        &mut self,
        request: GpuEngineRowsRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().system().gpu_engine_rows(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }

    /// Submit an NPU accelerator inventory read (capability
    /// `accelerator.npu`). The provider performs ONE bounded enumeration per
    /// request; a sorted device list (possibly empty — the honest no-NPU
    /// host) or a typed failure arrives as an `NpuInventoryEvent` publication
    /// in the next event batch.
    pub fn submit_npu_inventory(
        &mut self,
        request: NpuInventoryRequest,
        submitted_at_ms: u64,
    ) -> Result<RequestId, SubmissionError> {
        let id = self.request_ids.next_id();
        submit_request(
            id,
            self.handle.facets().system().npu_inventory(),
            submitted_at_ms,
            request,
        )?;
        Ok(id)
    }
}
