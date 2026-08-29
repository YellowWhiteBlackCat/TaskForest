//! Pure diagnostic-plan preparation and typed background publication port.

use std::path::PathBuf;

use taskmanager_core::core::services::ServiceLogEntry;
use taskmanager_core::{DiagnosticBundleError, DiagnosticBundleErrorKind, DiagnosticBundlePlan};

/// Prepare a privacy-safe service-log export using the same diagnostic bundle
/// redaction contract as the full diagnostics surface.
pub fn prepare_service_log_bundle(
    entries: &[ServiceLogEntry],
) -> Result<DiagnosticBundlePlan, DiagnosticBundleError> {
    let contents = serde_json::to_string_pretty(entries).map_err(|error| {
        DiagnosticBundleError::with_detail(DiagnosticBundleErrorKind::Encode, error.to_string())
    })?;
    DiagnosticBundlePlan::prepare(
        vec![taskmanager_core::DiagnosticSource {
            name: "service-logs.json".into(),
            contents,
        }],
        [],
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct DiagnosticBundleRequestId(u64);

impl DiagnosticBundleRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DiagnosticBundleTarget {
    CurrentDirectory { file_name: String },
    Path(PathBuf),
}

impl DiagnosticBundleTarget {
    #[must_use]
    pub fn current_directory(file_name: impl Into<String>) -> Self {
        Self::CurrentDirectory {
            file_name: file_name.into(),
        }
    }

    #[must_use]
    pub fn path(path: impl Into<PathBuf>) -> Self {
        Self::Path(path.into())
    }
}

#[derive(Clone, Debug)]
pub struct DiagnosticBundleRequest {
    id: DiagnosticBundleRequestId,
    plan: DiagnosticBundlePlan,
    target: DiagnosticBundleTarget,
}

impl DiagnosticBundleRequest {
    #[must_use]
    pub const fn id(&self) -> DiagnosticBundleRequestId {
        self.id
    }

    #[must_use]
    pub const fn plan(&self) -> &DiagnosticBundlePlan {
        &self.plan
    }

    #[must_use]
    pub const fn target(&self) -> &DiagnosticBundleTarget {
        &self.target
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DiagnosticBundleCompletion {
    pub request: DiagnosticBundleRequestId,
    pub destination: PathBuf,
    pub result: Result<(), DiagnosticBundleError>,
}

pub trait DiagnosticBundlePort {
    fn try_submit(&mut self, request: DiagnosticBundleRequest)
    -> Result<(), DiagnosticBundleError>;
    fn drain(&mut self) -> Vec<DiagnosticBundleCompletion>;
}

/// Couples one caller's admission credit and completion identity. One active
/// request per client prevents duplicate confirms; a completion received after
/// `close` or after a newer request is ignored.
#[derive(Debug)]
pub struct DiagnosticBundleSession<P> {
    port: P,
    active: Option<DiagnosticBundleRequestId>,
    next_request: Option<u64>,
}

impl<P: DiagnosticBundlePort> DiagnosticBundleSession<P> {
    #[must_use]
    pub fn new(port: P) -> Self {
        Self {
            port,
            active: None,
            next_request: Some(1),
        }
    }

    #[must_use]
    pub const fn active_request(&self) -> Option<DiagnosticBundleRequestId> {
        self.active
    }

    pub fn submit(
        &mut self,
        plan: DiagnosticBundlePlan,
        target: DiagnosticBundleTarget,
    ) -> Result<DiagnosticBundleRequestId, DiagnosticBundleError> {
        if self.active.is_some() {
            return Err(DiagnosticBundleError::new(DiagnosticBundleErrorKind::Busy));
        }
        let Some(next) = self.next_request else {
            return Err(DiagnosticBundleError::new(
                DiagnosticBundleErrorKind::Unavailable,
            ));
        };
        self.next_request = next.checked_add(1);
        let id = DiagnosticBundleRequestId(next);
        let request = DiagnosticBundleRequest { id, plan, target };
        self.port.try_submit(request)?;
        self.active = Some(id);
        Ok(id)
    }

    pub fn drain(&mut self) -> Vec<DiagnosticBundleCompletion> {
        let mut accepted = Vec::new();
        for completion in self.port.drain() {
            if self.active == Some(completion.request) {
                self.active = None;
                accepted.push(completion);
            }
        }
        accepted
    }

    pub fn close(&mut self) {
        self.active = None;
    }
}

#[cfg(test)]
#[path = "../tests/headless/application_diagnostics_tests.rs"]
mod tests;
