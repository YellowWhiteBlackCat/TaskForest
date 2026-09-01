//! The warm-up spinner: one accent arc revolving around a faint track while
//! the shell has not yet committed its first telemetry frame — the iced
//! counterpart of the GPUI 800 ms spinner, built on the same `Canvas` +
//! `Program` edge as the other charts.
//!
//! The revolution is app-state driven, not renderer driven: the per-frame
//! pump ([`crate::app::IcedApp::advance_motion`]) advances the phase, the
//! view passes it in, and this program only projects it onto geometry. A
//! `None` phase (ready shell or the no-motion policy) draws the same arc at
//! rest — a stable static glyph, never a fake rotation.
//!
//! Geometry is rebuilt every draw on purpose: the arc moves every frame
//! while spinning, so a [`canvas::Cache`] would be cleared anyway, and the
//! shape is two tiny strokes (cheaper than the fingerprint bookkeeping the
//! data-bearing charts need).

use iced::mouse;
use iced::widget::canvas::{self, Geometry, Path, Stroke};
use iced::{Color, Point, Radians, Rectangle};

use crate::app::Message;

/// The spinner's square canvas side (the warm-up card's glyph slot). A
/// cross-frontend layout contract, not a spacing token: the canvas extent
/// mirrors the warm-up card's glyph slot and no SPACE_* tier sits at 28.
pub(crate) const SPINNER_SIZE: f32 = 28.0;
/// Arc stroke weight (matches the GPUI spinner's band; layout contract —
/// the stroke ladder has no token on the shared scale).
const STROKE_WIDTH: f32 = 3.0;
/// The arc's angular sweep: three quarters of a revolution, with round caps.
const ARC_SWEEP: f32 = 1.5 * std::f32::consts::PI;

/// The revolving warm-up spinner. `phase` (0..1) is the arc head's position
/// in the current revolution; `None` renders the arc at rest.
pub(crate) struct WarmupSpinner {
    accent: Color,
    track: Color,
    phase: Option<f32>,
}

impl WarmupSpinner {
    /// Build the spinner from the neutral theme snapshot and the current
    /// revolution phase (`None` = static arc). Both stroke colors are
    /// palette-token-bound — the accent token for the arc, the border token
    /// for the faint track — so every skin restyles the spinner for free.
    pub(crate) fn new(theme: &taskmanager_theme::Theme, phase: Option<f32>) -> Self {
        Self {
            accent: crate::theme_binding::color(theme.palette().accent),
            track: crate::theme_binding::color(theme.palette().border),
            phase,
        }
    }
}

impl canvas::Program<Message> for WarmupSpinner {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &iced::Renderer,
        _theme: &iced::Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let size = bounds.size();
        let mut frame = canvas::Frame::new(renderer, size);
        let radius = (size.width.min(size.height) - STROKE_WIDTH) * 0.5;
        let center = Point::new(size.width * 0.5, size.height * 0.5);
        let mut track = Stroke::default()
            .with_width(STROKE_WIDTH)
            .with_color(self.track);
        track.line_cap = canvas::LineCap::Round;
        let mut arc = Stroke::default()
            .with_width(STROKE_WIDTH)
            .with_color(self.accent);
        arc.line_cap = canvas::LineCap::Round;

        frame.stroke(&Path::circle(center, radius), track);
        let (start, end) = arc_span(self.phase.unwrap_or(0.0));
        frame.stroke(
            &Path::new(|builder| {
                builder.arc(canvas::path::Arc {
                    center,
                    radius,
                    start_angle: Radians(start),
                    end_angle: Radians(end),
                });
            }),
            arc,
        );
        vec![frame.into_geometry()]
    }
}

/// The arc's angular span for one phase (0..1): the head sits at
/// `phase * 2π` measured clockwise from twelve o'clock and trails
/// [`ARC_SWEEP`] behind it, so the phase→angle projection is a pure seam
/// the headless tests can pin (monotone rotation, fixed sweep, wrap at one
/// revolution).
#[must_use]
pub(crate) fn arc_span(phase: f32) -> (f32, f32) {
    let head = -0.5 * std::f32::consts::PI + phase * 2.0 * std::f32::consts::PI;
    (head - ARC_SWEEP, head)
}

/// The fixed square `Canvas` for the warm-up card. Kept beside the program so
/// the warm-up body assembles one spinner, never ad-hoc canvas literals.
pub(crate) fn canvas_view(
    theme: &taskmanager_theme::Theme,
    phase: Option<f32>,
) -> iced::Element<'_, Message> {
    canvas::Canvas::new(WarmupSpinner::new(theme, phase))
        .width(SPINNER_SIZE)
        .height(SPINNER_SIZE)
        .into()
}
