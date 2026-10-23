//! Diagnostic review and correlated background export adapter.
//!
//! The module also owns the report SKELETON (ICED-050): while the
//! diagnostic bundle is actually being written, the modal shows a wave
//! placeholder instead of a frozen panel. The placeholder is driven by the
//! REAL in-flight state (the `Writing` variant of the surface's typed
//! lifecycle), so it can never cover a preview, a finished report or a
//! failure — and it disappears the moment the write settles.

use iced::widget::{column, container, row};
use iced::{Element, Length};
use taskmanager_app_host::DiagnosticBundleClient;
use taskmanager_application::diagnostics::DiagnosticBundleUiState;
use taskmanager_application::{DiagnosticBundleSession, DiagnosticBundleTarget};
use taskmanager_theme::Theme;

use super::{IcedApp, LocalSurface, Message};

/// How many placeholder bands the skeleton draws.
pub(crate) const DIAGNOSTIC_SKELETON_ROWS: usize = 5;
/// The wave's speed in cycles per second.
pub(crate) const DIAGNOSTIC_SKELETON_WAVE_HZ: f32 = 0.8;
/// The placeholder band's height in px (a layout contract, not a token).
pub(crate) const DIAGNOSTIC_SKELETON_ROW_HEIGHT: f32 = 10.0;

/// One placeholder band of the skeleton.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SkeletonRow {
    /// The band's width share of the placeholder area (0..1).
    pub width: f32,
    /// The band's phase on the traveling wave (0..1).
    pub phase: f32,
}

/// The diagnostics skeleton: the placeholder bands drawn while the report
/// is being written.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DiagnosticSkeleton {
    /// The placeholder bands.
    pub rows: Vec<SkeletonRow>,
}

/// The skeleton for one diagnostics surface state, or `None` when the real
/// content is on screen.
///
/// The placeholder is bound to the IN-FLIGHT state: only the `Writing`
/// variant — the state the surface reaches after the user confirmed the
/// export and before the write settles — carries the skeleton. A preview, a
/// finished report and a failure all render their own content.
#[must_use]
pub(crate) fn diagnostic_skeleton(state: &DiagnosticBundleUiState) -> Option<DiagnosticSkeleton> {
    if !matches!(state, DiagnosticBundleUiState::Writing(_)) {
        return None;
    }
    Some(DiagnosticSkeleton {
        rows: (0..DIAGNOSTIC_SKELETON_ROWS)
            .map(|index| SkeletonRow {
                // The bands taper like the preview's own lines: the longest
                // one first, each following one shorter.
                width: 0.92 - (index as f32) * 0.14,
                phase: index as f32 / DIAGNOSTIC_SKELETON_ROWS as f32,
            })
            .collect(),
    })
}

/// The wave's brightness at one phase: a smooth band that travels across
/// the placeholder rows. The value stays inside 0..1 for every phase, so a
/// band's alpha never leaves its range.
#[must_use]
pub(crate) fn skeleton_wave(phase: f32) -> f32 {
    let phase = phase.rem_euclid(1.0);
    (phase * std::f32::consts::TAU).cos() * 0.5 + 0.5
}

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

/// The placeholder bands of the skeleton, painted with the traveling wave.
fn diagnostic_skeleton_element<'a>(
    theme_snapshot: &Theme,
    skeleton: &DiagnosticSkeleton,
    phase: f32,
) -> Element<'a, Message, iced::Theme, iced::Renderer> {
    let base = crate::theme_binding::color(theme_snapshot.palette().border);
    let mut rows = column![];
    for band in &skeleton.rows {
        let wave = skeleton_wave(band.phase + phase);
        let mut fill = base;
        fill.a = 0.22 + 0.5 * wave;
        // The band's share of the row: the placeholder line and the rest of
        // the row split the row's fill portions, so the line really takes
        // its own share of the modal's width.
        let band_portion = (band.width * 100.0).round().clamp(1.0, 99.0) as u16;
        let rest_portion = 100_u16.saturating_sub(band_portion);
        rows = rows.push(
            row![
                container(iced::widget::Space::new())
                    .width(Length::FillPortion(band_portion))
                    .height(Length::Fixed(DIAGNOSTIC_SKELETON_ROW_HEIGHT))
                    .style(move |_| crate::theme::fill_style(fill)),
                iced::widget::Space::new().width(Length::FillPortion(rest_portion)),
            ]
            .width(Length::Fill),
        );
    }
    rows.spacing(6).width(Length::Fill).into()
}

impl IcedApp {
    /// The diagnostics skeleton placeholder for the current surface, or
    /// `None` when the surface is not an in-flight report.
    ///
    /// The wave's phase comes from the tick-injected clock (the view never
    /// reads the wall clock), so the placeholder advances with the
    /// frontend's own tick.
    #[must_use]
    pub fn diagnostic_skeleton_view(
        &self,
    ) -> Option<Element<'_, Message, iced::Theme, iced::Renderer>> {
        let Some(LocalSurface::DiagnosticBundle(state)) = self.local_surface() else {
            return None;
        };
        let skeleton = diagnostic_skeleton(state)?;
        Some(diagnostic_skeleton_element(
            self.theme(),
            &skeleton,
            self.diagnostic_skeleton_phase(),
        ))
    }

    /// The skeleton's wave phase, derived from the tick-injected clock.
    fn diagnostic_skeleton_phase(&self) -> f32 {
        let seconds = self.service_log_now_micros() as f32 / 1_000_000.0;
        (seconds * DIAGNOSTIC_SKELETON_WAVE_HZ).rem_euclid(1.0)
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

#[cfg(test)]
#[path = "../../tests/gui/app/diagnostics_skeleton_tests.rs"]
mod skeleton_tests;
