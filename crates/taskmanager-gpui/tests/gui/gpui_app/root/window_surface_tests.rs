use super::*;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::target::ServiceId;
use taskmanager_core::core::{DiagnosticBundleError, DiagnosticBundleErrorKind};

fn surface_fixture(kind: WindowSurfaceKind) -> WindowSurface {
    match kind {
        WindowSurfaceKind::Settings => WindowSurface::Settings,
        WindowSurfaceKind::Help => WindowSurface::Help,
        WindowSurfaceKind::SystemAbout => WindowSurface::SystemAbout,
        WindowSurfaceKind::About => WindowSurface::About,
        WindowSurfaceKind::FirstRun => WindowSurface::FirstRun,
        WindowSurfaceKind::RunTask => WindowSurface::RunTask,
        WindowSurfaceKind::DiagnosticBundle => {
            WindowSurface::DiagnosticBundle(DiagnosticBundleUiState::Failed(
                DiagnosticBundleError::new(DiagnosticBundleErrorKind::Io),
            ))
        }
        WindowSurfaceKind::ServiceDetails => {
            WindowSurface::ServiceDetails(ServiceId::new("fixture.service"))
        }
        WindowSurfaceKind::DiskSmart => WindowSurface::DiskSmart(2),
        WindowSurfaceKind::DashboardPanel => WindowSurface::DashboardPanel(DashboardPanel::Events),
        WindowSurfaceKind::ProcessAffinity => WindowSurface::ProcessAffinity(
            ProcessLiveKey::from_parts(41, 41).expect("fixture identity"),
        ),
    }
}

#[test]
fn branch_registry_maps_every_payload_to_one_exact_kind() {
    let surfaces = WindowSurfaceKind::ALL.map(surface_fixture);
    let mapped = surfaces.iter().map(WindowSurface::kind).collect::<Vec<_>>();
    assert_eq!(mapped.as_slice(), WindowSurfaceKind::ALL.as_slice());
}

#[test]
fn replacement_and_stale_dismiss_preserve_one_owner() {
    let mut state = WindowSurfaceState::default();
    assert_eq!(
        state.reduce(WindowSurfaceEvent::Open(WindowSurface::Settings)),
        WindowSurfaceTransition::Opened(WindowSurfaceKind::Settings)
    );
    assert_eq!(
        state.reduce(WindowSurfaceEvent::Open(WindowSurface::Help)),
        WindowSurfaceTransition::Replaced {
            previous: WindowSurfaceKind::Settings,
            current: WindowSurfaceKind::Help,
        }
    );
    assert_eq!(
        state.reduce(WindowSurfaceEvent::Dismiss {
            expected: WindowSurfaceKind::Settings,
            reason: WindowSurfaceDismissReason::CloseButton,
        }),
        WindowSurfaceTransition::Unchanged
    );
    assert_eq!(state.kind(), Some(WindowSurfaceKind::Help));
    assert_eq!(
        state.reduce(WindowSurfaceEvent::Dismiss {
            expected: WindowSurfaceKind::Settings,
            reason: WindowSurfaceDismissReason::Cancel,
        }),
        WindowSurfaceTransition::Unchanged,
        "a stale close branch cannot consume the current surface"
    );
    assert_eq!(state.kind(), Some(WindowSurfaceKind::Help));
    assert!(matches!(
        state.reduce(WindowSurfaceEvent::Dismiss {
            expected: WindowSurfaceKind::Help,
            reason: WindowSurfaceDismissReason::Cancel,
        }),
        WindowSurfaceTransition::Dismissed {
            surface: WindowSurfaceKind::Help,
            ..
        }
    ));
    assert_eq!(state.kind(), None);
}
