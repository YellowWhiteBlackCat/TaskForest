//! Typed state machine for detaching one Performance chart into its own window.
//!
//! This module owns the *decision* and lifecycle of a detached chart request.
//! It does not create GPUI windows, query the compositor, or claim a Wayland
//! protocol is available. The pinned GPUI snapshot exposes no public API for
//! creating a second, compositor-owned floating chart surface from this
//! frontend, so [`DetachedChartWindowState::capability`] is
//! [`ChartWindowCapability::Unavailable`] on every platform until a real
//! window-opening adapter is wired. A UI caller must refuse a detach request
//! before a window is ever requested; hosting a chart inside another window is
//! never silently substituted for a genuinely detached chart.

use std::fmt;

/// A frozen Performance chart identity for one detach session.
///
/// The device identity remains the authority for the chart's facts; this key
/// only names which declaration the user asked to detach. It carries no
/// samples, renderer state, or OS window handle.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct DetachedChartKey {
    device: String,
    chart: String,
}

impl DetachedChartKey {
    /// Freeze one chart declaration. Empty identities are rejected so an
    /// unavailable request can never be reported as a detached chart.
    pub fn new(
        device: impl Into<String>,
        chart: impl Into<String>,
    ) -> Result<Self, DetachedChartKeyError> {
        let device = device.into();
        let chart = chart.into();
        if device.trim().is_empty() || chart.trim().is_empty() {
            return Err(DetachedChartKeyError::EmptyIdentity);
        }
        Ok(Self { device, chart })
    }

    /// The frozen device identity.
    #[must_use]
    pub fn device(&self) -> &str {
        &self.device
    }

    /// The frozen chart identity within the device.
    #[must_use]
    pub fn chart(&self) -> &str {
        &self.chart
    }
}

/// Why a detached chart request was refused.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetachedChartWindowError {
    /// No real second-window adapter exists in this product build.
    CapabilityUnavailable,
    /// The close did not name the chart that owns the active detach session.
    NotDetached,
}

impl fmt::Display for DetachedChartWindowError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CapabilityUnavailable => {
                formatter.write_str("detached chart windows are unavailable on this host")
            }
            Self::NotDetached => formatter.write_str("that chart is not detached"),
        }
    }
}

impl std::error::Error for DetachedChartWindowError {}

/// Why a [`DetachedChartKey`] could not be constructed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DetachedChartKeyError {
    /// One identity was empty or whitespace only.
    EmptyIdentity,
}

impl fmt::Display for DetachedChartKeyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyIdentity => formatter.write_str("chart identity cannot be empty"),
        }
    }
}

impl std::error::Error for DetachedChartKeyError {}

/// How the product may host a chart outside the main Performance viewport.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ChartWindowDetachMode {
    /// The chart stays in the main Performance page.
    #[default]
    Embedded,
    /// The chart is requested as a separate normal desktop window.
    SeparateWindow,
    /// The chart is requested as a Wayland layer-shell surface.
    ///
    /// This is only a typed request. No layer-shell capability discovery is
    /// performed by this module, and the current product build reports the
    /// mode unavailable rather than pretending the protocol was negotiated.
    LayerShellSurface,
}

impl ChartWindowDetachMode {
    /// Whether this mode requests a surface outside the main window.
    #[must_use]
    pub const fn is_detached(self) -> bool {
        !matches!(self, Self::Embedded)
    }
}

/// Honest, build-scoped capability of the detached-chart-window adapter.
///
/// There is deliberately no `Available` variant in this slice: adding one
/// requires a real composition adapter that can open and close a second
/// window and prove its protocol role. Until then, every detach attempt fails
/// closed without changing state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChartWindowCapability {
    /// No second-window adapter is wired in this build.
    Unavailable { reason: &'static str },
}

impl ChartWindowCapability {
    /// The build-scoped capability fact. Never inferred from environment
    /// variables, host snapshots, or a simulated success.
    #[must_use]
    pub const fn current() -> Self {
        Self::Unavailable {
            reason: "the pinned GPUI snapshot has no detached chart window adapter",
        }
    }

    /// A human-readable reason for the unavailable state.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::Unavailable { reason } => reason,
        }
    }
}

/// One typed command submitted by a UI caller.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DetachedChartWindowCommand {
    /// Request a chart to enter a detach mode.
    Detach {
        /// The frozen chart declaration.
        key: DetachedChartKey,
        /// The requested host mode.
        mode: ChartWindowDetachMode,
    },
    /// Close the current detach session and return the chart to the page.
    Reattach {
        /// The chart whose session should close.
        key: DetachedChartKey,
    },
}

/// Pure, window-local authority for detached chart requests.
///
/// `None` means the chart is embedded. `Some` records only a *granted*
/// request; because the current capability is unavailable, production state
/// cannot leave `None` through [`Self::apply`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DetachedChartWindowState {
    detached: Option<(DetachedChartKey, ChartWindowDetachMode)>,
}

impl DetachedChartWindowState {
    /// Construct the embedded default.
    #[must_use]
    pub const fn new() -> Self {
        Self { detached: None }
    }

    /// The adapter capability for this build.
    #[must_use]
    pub fn capability(&self) -> ChartWindowCapability {
        ChartWindowCapability::current()
    }

    /// The currently detached chart, if a real adapter granted one.
    #[must_use]
    pub fn detached(&self) -> Option<(&DetachedChartKey, ChartWindowDetachMode)> {
        self.detached.as_ref().map(|(key, mode)| (key, *mode))
    }

    /// Whether one chart owns an active detach session.
    #[must_use]
    pub const fn is_detached(&self) -> bool {
        self.detached.is_some()
    }

    /// Apply one typed command. On an unavailable adapter, every detach is
    /// refused without mutating state; a reattach with no session is also a
    /// typed refusal. This is idempotent: replaying the same refused command
    /// produces the same error and the same state.
    pub fn apply(
        &mut self,
        command: DetachedChartWindowCommand,
    ) -> Result<ChartWindowDetachMode, DetachedChartWindowError> {
        match command {
            DetachedChartWindowCommand::Detach { key, mode } => {
                let ChartWindowCapability::Unavailable { .. } = self.capability();
                if mode == ChartWindowDetachMode::Embedded {
                    return self.reattach(&key);
                }
                if self.detached.is_some() {
                    return self.reattach(&key);
                }
                Err(DetachedChartWindowError::CapabilityUnavailable)
            }
            DetachedChartWindowCommand::Reattach { key } => self.reattach(&key),
        }
    }

    fn reattach(
        &mut self,
        key: &DetachedChartKey,
    ) -> Result<ChartWindowDetachMode, DetachedChartWindowError> {
        let Some((active, _)) = self.detached.as_ref() else {
            return Err(DetachedChartWindowError::NotDetached);
        };
        if active != key {
            return Err(DetachedChartWindowError::NotDetached);
        }
        self.detached = None;
        Ok(ChartWindowDetachMode::Embedded)
    }
}

#[cfg(test)]
#[path = "../tests/headless/detached_chart_window_tests.rs"]
mod tests;
