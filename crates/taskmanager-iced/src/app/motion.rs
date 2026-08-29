//! Frontend-local motion state for the Iced adapter: the modal-entrance
//! animation and the warm-up spinner phase. Extracted from [`super`] so the
//! state module stays under the repository's source-size budget.
//!
//! The entrance runs on iced's own animation engine (`iced::Animation`, a
//! lilt-backed easing interpolator) instead of a hand-rolled linear ramp: the
//! tick (and the per-frame pump while anything animates) advances it, the
//! renderer reads the eased progress — never a clock.

use std::sync::atomic::{AtomicU8, Ordering};
use std::time::{Duration, Instant};

use taskmanager_application::AppPage;
use taskmanager_theme::tokens::{DURATION_MEDIUM, MotionPolicy};

/// The process-wide motion policy for the iced frontend, stored as state
/// (never a call-site literal) so the single seam below can repoint it.
///
/// The install source is the shared `Config::motion` preference token
/// ("normal" / "reduced" / "none", the core `MOTION_*` vocabulary): every
/// configuration snapshot application seeds the policy through
/// [`install_motion_policy`], and every animation follows. iced 0.14 and its
/// winit 0.30 backend still expose no OS prefers-reduced-motion capability
/// (no window event, no `window::Settings` field), so the config preference
/// is the one honest switch — no OS-derived value is fabricated on top of it.
///
/// Process-global is honest here: the OS motion preference is desktop-wide
/// environment state, the iced product is single-instance (the launcher's
/// instance guard), and `IcedApp` owns the one window — there is no second
/// app this state could cross. Headless tests share the global, so only the
/// runtime edges (config application) write it, never the constructors.
static MOTION_POLICY: AtomicU8 = AtomicU8::new(MotionPolicy::Normal as u8);

/// The currently installed motion policy.
#[must_use]
pub(crate) fn motion_policy_state() -> MotionPolicy {
    match MOTION_POLICY.load(Ordering::Relaxed) {
        1 => MotionPolicy::Reduced,
        2 => MotionPolicy::NoMotion,
        _ => MotionPolicy::Normal,
    }
}

/// Install the process-wide motion policy (one configuration snapshot's
/// resolved preference). Relaxed ordering is sufficient: the value is a
/// plain enum carried by value and the next animation start re-reads it.
pub(crate) fn install_motion_policy(policy: MotionPolicy) {
    MOTION_POLICY.store(policy as u8, Ordering::Relaxed);
}

/// The persisted `Config::motion` token for one policy — the shared core
/// `MOTION_NORMAL` / `MOTION_REDUCED` / `MOTION_NONE` vocabulary.
#[must_use]
pub(crate) const fn motion_token(policy: MotionPolicy) -> &'static str {
    match policy {
        MotionPolicy::Normal => "normal",
        MotionPolicy::Reduced => "reduced",
        MotionPolicy::NoMotion => "none",
    }
}

/// Parse a persisted `Config::motion` token into the shared policy. The
/// token comparison is case-insensitive; an empty or unknown token degrades
/// to [`MotionPolicy::Normal`] (the pre-preference animated default) — never
/// a panic and never a fabricated stronger restriction.
#[must_use]
pub(crate) fn motion_policy_from_token(token: &str) -> MotionPolicy {
    match token.trim().to_ascii_lowercase().as_str() {
        "reduced" => MotionPolicy::Reduced,
        "none" | "no-motion" => MotionPolicy::NoMotion,
        _ => MotionPolicy::Normal,
    }
}

/// The modal-entrance animation state: the iced animation (easing engine)
/// plus the eased progress (0..1) the renderer reads.
#[derive(Clone, Debug)]
pub struct ModalAppear {
    animation: iced::Animation<bool>,
    pub(crate) progress: f32,
}

impl ModalAppear {
    /// The entrance duration class: the shared appear/panel token (180 ms),
    /// the same class the GPUI modal fade uses.
    pub const DURATION: Duration = DURATION_MEDIUM;

    /// Start a new entrance at `now` under the shared motion policy. The
    /// eased sweep uses iced's animation engine (`EaseOutCubic` — a gentle
    /// decelerating fade); [`MotionPolicy::NoMotion`] skips the sweep and
    /// starts at the final state so no frame pump is ever needed.
    #[must_use]
    pub fn new(policy: MotionPolicy, now: Instant) -> Self {
        match policy.animation_duration(Self::DURATION) {
            Some(duration) => Self {
                animation: iced::Animation::new(false)
                    .duration(duration)
                    .easing(iced::animation::Easing::EaseOutCubic)
                    .go(true, now),
                progress: 0.0,
            },
            None => Self {
                animation: iced::Animation::new(true),
                progress: 1.0,
            },
        }
    }

    /// Advance the eased progress to `now`. Pure so the headless tests assert
    /// the eased ramp without a live frame loop; a completed entrance stays
    /// clamped at 1.0.
    #[must_use]
    pub fn advance(mut self, now: Instant) -> Self {
        self.progress = self
            .animation
            .interpolate(0.0_f32, 1.0_f32, now)
            .clamp(0.0, 1.0);
        self
    }

    /// The eased entrance progress (0..1); headless assertions read this.
    #[must_use]
    pub fn progress(&self) -> f32 {
        self.progress
    }
}

/// The warm-up spinner phase: one accent arc revolving around a faint track
/// while the shell has not yet committed its first telemetry frame. This is
/// the iced counterpart of the GPUI 800 ms spinner; the per-frame pump
/// advances it, the renderer reads the phase — never a clock.
#[derive(Clone, Copy, Debug)]
pub struct WarmupSpin {
    started: Instant,
    pub(crate) phase: f32,
}

impl WarmupSpin {
    /// One full revolution of the spinner arc (matches the GPUI spinner).
    pub const PERIOD: Duration = Duration::from_millis(800);

    /// Start revolving at `now`. `None` under [`MotionPolicy::NoMotion`]:
    /// the caller renders a static arc and never pumps frames. The spinner
    /// is a continuous progress indicator, not a decorative transition, so
    /// `Reduced` keeps revolving (only the sweep-free policies freeze it).
    #[must_use]
    pub fn new(policy: MotionPolicy, now: Instant) -> Option<Self> {
        matches!(policy, MotionPolicy::Normal | MotionPolicy::Reduced).then_some(Self {
            started: now,
            phase: 0.0,
        })
    }

    /// Advance the phase to `now`: `(elapsed % PERIOD) / PERIOD` in 0..1,
    /// so the arc revolves forever without accumulating drift. Pure so the
    /// headless tests assert the wrap without a live frame loop.
    #[must_use]
    pub fn advance(mut self, now: Instant) -> Self {
        let elapsed = now.saturating_duration_since(self.started).as_millis() as f32;
        let period = Self::PERIOD.as_millis() as f32;
        self.phase = (elapsed % period) / period;
        self
    }

    /// The current revolution phase (0..1); headless assertions read this.
    #[must_use]
    pub fn phase(&self) -> f32 {
        self.phase
    }
}

/// Whether a viewport is at the GPUI compact breakpoint
/// (`width <= 820.0 || height <= 540.0`), mirroring the compact layout profile
/// exactly so the iced Performance page collapses to its narrow layout at the
/// same window sizes GPUI does — including a short, wide window (a docked
/// panel). Pure so the headless policy test mirrors `responsive::tests`
/// without a live window.
#[must_use]
pub(crate) fn viewport_compact(size: iced::Size) -> bool {
    size.width <= 820.0 || size.height <= 540.0
}

/// Page navigation from a tab click.
#[derive(Debug, Clone, Copy)]
pub struct PageNav(pub AppPage);

impl crate::IcedApp {
    /// The motion policy the iced frontend currently runs: the installed
    /// configuration preference delivered through [`motion_policy_state`] —
    /// never a literal at the call sites — so one snapshot application (or a
    /// Settings change committing a new snapshot) repoints every animation
    /// in one place (see the `MOTION_POLICY` contract above).
    pub(crate) fn motion_policy(&self) -> MotionPolicy {
        motion_policy_state()
    }

    /// Advance every frontend-local motion state to `now` and maintain the
    /// spinner lifecycle: the spinner exists exactly while the shell is
    /// still collecting its first telemetry frame. Called from both the
    /// frame pump (per frame while animating) and the tick (a coarse
    /// fallback so entrances always complete even without frames).
    pub(crate) fn advance_motion(&mut self, now: Instant) {
        if let Some(appear) = self.input.modal_appear.take() {
            self.input.modal_appear = Some(appear.advance(now));
        }
        let collecting = self.shell.telemetry_frame_state().is_collecting();
        self.input.warmup_spin = match (collecting, self.input.warmup_spin.take()) {
            (true, None) => WarmupSpin::new(self.motion_policy(), now),
            (true, Some(spin)) => Some(spin.advance(now)),
            (false, _) => None,
        };
    }

    /// Whether frontend-local motion needs iced's per-frame pump this cycle:
    /// the capture marker, an unfinished modal entrance, or the warm-up
    /// spinner. While false, the runtime idles between events — the pump is
    /// subscribed only for the frames an animation actually needs.
    pub(crate) fn frame_pump_active(&self) -> bool {
        (self.capture.marker.is_some() && !self.capture.emitted)
            || self
                .input
                .modal_appear
                .as_ref()
                .is_some_and(|appear| appear.progress < 1.0)
            || (self.input.warmup_spin.is_some()
                && self.shell.telemetry_frame_state().is_collecting())
    }

    /// The warm-up spinner phase the renderer draws (`None` while the shell
    /// is ready or the policy froze the arc — callers render a static arc).
    #[must_use]
    pub fn warmup_spin_phase(&self) -> Option<f32> {
        (self.input.warmup_spin.is_some() && self.shell.telemetry_frame_state().is_collecting())
            .then(|| self.input.warmup_spin.map_or(0.0, |spin| spin.phase))
    }

    /// Queue an effect through the platform client (or record the demo
    /// suppression honestly).
    pub fn queue(&mut self, effect: crate::app::PlatformEffect) {
        let details_query = match &effect {
            crate::app::PlatformEffect::ServiceLogStream(request)
                if self.service_details_target() == Some(&request.query.service_id) =>
            {
                Some(request.query.clone())
            }
            _ => None,
        };
        let shell_log_query = match &effect {
            crate::app::PlatformEffect::ServiceLogStream(request)
                if self
                    .shell
                    .service_log
                    .as_ref()
                    .and_then(|open| open.service_id())
                    == Some(&request.query.service_id) =>
            {
                Some(request.query.clone())
            }
            _ => None,
        };
        let details_attempt = details_query
            .clone()
            .and_then(|query| self.service_details.begin_stream_attempt(query));
        match self.runtime.queue(&mut self.shell, effect) {
            Ok(request_ids) => {
                if let (Some(attempt_id), Some(request_id)) =
                    (details_attempt, request_ids.into_iter().next())
                {
                    self.service_details.accept_stream(attempt_id, request_id);
                }
            }
            Err(error) => {
                let failure = taskmanager_application::service_submission_failure(error);
                if let Some(attempt_id) = details_attempt {
                    self.service_details.reject_stream(attempt_id, failure);
                }
                if let (Some(query), Some(open)) =
                    (shell_log_query, self.shell.service_log.as_mut())
                    && open.lifecycle.failure().is_none()
                    && let Some(attempt_id) = open.lifecycle.begin_attempt(query)
                {
                    open.lifecycle.reject_attempt(
                        attempt_id,
                        taskmanager_core::core::services::ServiceLogFailure::with_detail(
                            taskmanager_core::core::services::ServiceLogErrorKind::from_failure(
                                failure,
                            ),
                            "service log request submission failed",
                        ),
                    );
                }
            }
        }
    }
}
