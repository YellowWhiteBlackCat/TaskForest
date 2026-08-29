//! Platform-neutral control requests and their asynchronous outcomes.

use taskmanager_core::core::failure::FailureKind;
use taskmanager_core::core::services::ServiceAction;
use taskmanager_core::core::session::SessionControlAction;
use taskmanager_core::core::startup::{StartupEntry, StartupEntryId};
use taskmanager_core::core::target::{ServiceId, SessionId};

/// Correlates an asynchronous control result with the latest UI intent.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ControlRequestId(u64);

impl ControlRequestId {
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// Allocates control request ids and rejects results superseded by a newer intent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatestControlRequest {
    next: u64,
    pending: Option<ControlRequestId>,
}

/// Latest-wins service intent correlation including exact target authority.
///
/// A generation match alone is insufficient at a native control boundary:
/// malformed or stale completions must also echo the provider-issued target and
/// action of the accepted UI intent.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LatestServiceControlRequest {
    generation: LatestControlRequest,
    pending: Option<(ControlRequestId, ServiceId, ServiceAction)>,
}

impl LatestServiceControlRequest {
    #[must_use]
    pub fn begin(&mut self, service_id: ServiceId, action: ServiceAction) -> ControlRequestId {
        let request_id = self.generation.begin();
        self.pending = Some((request_id, service_id, action));
        request_id
    }

    pub fn accept(
        &mut self,
        request_id: ControlRequestId,
        service_id: &ServiceId,
        action: ServiceAction,
    ) -> bool {
        if !self.pending.as_ref().is_some_and(
            |(expected_request, expected_service, expected_action)| {
                *expected_request == request_id
                    && expected_service == service_id
                    && *expected_action == action
            },
        ) {
            return false;
        }
        if !self.generation.accept(request_id) {
            return false;
        }
        self.pending = None;
        true
    }

    #[must_use]
    pub fn pending(&self) -> Option<(ControlRequestId, &ServiceId, ServiceAction)> {
        self.pending
            .as_ref()
            .map(|(request_id, service_id, action)| (*request_id, service_id, *action))
    }
}

impl LatestControlRequest {
    #[must_use]
    pub fn begin(&mut self) -> ControlRequestId {
        self.next = self.next.wrapping_add(1);
        if self.next == 0 {
            self.next = 1;
        }
        let request_id = ControlRequestId(self.next);
        self.pending = Some(request_id);
        request_id
    }

    /// Accept only the latest pending result. Older completions cannot replace
    /// feedback for a newer request.
    pub fn accept(&mut self, request_id: ControlRequestId) -> bool {
        if self.pending != Some(request_id) {
            return false;
        }
        self.pending = None;
        true
    }

    #[must_use]
    pub const fn pending(&self) -> Option<ControlRequestId> {
        self.pending
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupControlRequest {
    pub request_id: ControlRequestId,
    pub entry: StartupEntry,
    pub enabled: bool,
}

/// Asynchronous result of one service lifecycle request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ServiceControlOutcome {
    pub request_id: ControlRequestId,
    pub service_id: ServiceId,
    pub action: ServiceAction,
    pub result: Result<(), FailureKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupControlOutcome {
    pub request_id: ControlRequestId,
    pub target_id: StartupEntryId,
    pub target_name: String,
    pub enabled: bool,
    pub result: Result<(), FailureKind>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionControlRequest {
    pub request_id: ControlRequestId,
    pub session_id: SessionId,
    pub action: SessionControlAction,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SessionControlOutcome {
    pub request_id: ControlRequestId,
    pub session_id: SessionId,
    pub action: SessionControlAction,
    pub result: Result<(), FailureKind>,
}

#[cfg(test)]
#[path = "../tests/headless/application_control_tests.rs"]
mod tests;
