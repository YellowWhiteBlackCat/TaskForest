//! Diagnostic review and correlated background export adapter.

use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};

use super::{IcedApp, LocalSurface};

#[derive(Debug, Default)]
pub(super) enum IcedDiagnosticRuntime {
    #[default]
    Unavailable,
    Active(DiagnosticBundleSession<DiagnosticBundleClient>),
}

impl IcedDiagnosticRuntime {
    pub(super) fn active_mut(
        &mut self,
    ) -> Option<&mut DiagnosticBundleSession<DiagnosticBundleClient>> {
        match self {
            Self::Unavailable => None,
            Self::Active(session) => Some(session),
        }
    }

    pub(super) fn close(&mut self) {
        if let Some(session) = self.active_mut() {
            session.close();
        }
    }
}

impl IcedApp {
    pub(crate) fn install_diagnostic_bundle_client(&mut self, client: DiagnosticBundleClient) {
        self.diagnostics = IcedDiagnosticRuntime::Active(DiagnosticBundleSession::new(client));
    }

    pub(crate) fn open_diagnostic_bundle(&mut self) {
        self.diagnostics.close();
        let state = DiagnosticBundleUiState::prepared(
            self.shell
                .projection()
                .prepare_diagnostic_bundle(Some(&self.config_draft())),
        );
        self.open_local_surface(LocalSurface::DiagnosticBundle(state));
    }

    pub(super) fn confirm_diagnostic_bundle(&mut self) {
        let target = DiagnosticBundleTarget::current_directory(format!(
            "taskmanager-diagnostics-{}.json",
            self.shell
                .projection()
                .snapshot
                .as_ref()
                .map_or(0, |snapshot| snapshot.timestamp_ms)
        ));
        let mut state = match self.local_surface() {
            Some(LocalSurface::DiagnosticBundle(state)) => state.clone(),
            _ => return,
        };
        if state.confirm(self.diagnostics.active_mut(), target) {
            let _ = self
                .local_surface
                .reduce(super::surface::LocalSurfaceEvent::Open(
                    LocalSurface::DiagnosticBundle(state),
                ));
        }
    }

    pub(super) fn drain_diagnostic_bundle_completions(&mut self) -> bool {
        let completions = self
            .diagnostics
            .active_mut()
            .map_or_else(Vec::new, DiagnosticBundleSession::drain);
        let mut changed = false;
        for completion in completions {
            if let Some(state) = self.local_surface.diagnostic_bundle_mut() {
                changed |= state.complete(completion);
            }
        }
        changed
    }

    pub(super) fn retry_diagnostic_bundle(&mut self) {
        if matches!(
            self.local_surface(),
            Some(LocalSurface::DiagnosticBundle(
                DiagnosticBundleUiState::Failed(_)
            ))
        ) {
            self.open_diagnostic_bundle();
        }
    }
}
