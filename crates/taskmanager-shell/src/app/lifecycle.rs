//! Typed shell lifecycle for quit intent and user-visible feedback.

use super::ShellApp;
use std::time::Duration;
use taskmanager_application::{
    ServiceControlOutcome, SessionControlOutcome, StartupControlOutcome,
};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum QuitState {
    #[default]
    Running,
    Requested(QuitReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitReason {
    Keyboard,
    CommandPalette,
    WindowClose,
    Tray,
    /// The platform already relaunched a replacement instance (the first-run
    /// setup script's Restart completed), so this instance exits without
    /// user-visible feedback — GPUI's post-restart `app.quit()` semantics.
    Restart,
}

/// Result of the one-way `Running -> Requested` transition.
///
/// The first reason remains authoritative. Later requests are observable but
/// cannot rewrite why shutdown began.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QuitRequestOutcome {
    Requested(QuitReason),
    AlreadyRequested(QuitReason),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackSource {
    Platform,
    Control,
    Settings,
    Clipboard,
    Navigation,
    Interaction,
    Persistence,
    Demo,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackSeverity {
    Info,
    Success,
    Warning,
    Error,
}

/// Explicit notice lifetime. `PlatformBatches(n)` survives until `n`
/// subsequently applied platform batches have crossed the shell reducer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FeedbackBatchLifetime(u8);

impl FeedbackBatchLifetime {
    const ONE: Self = Self(1);
    const TWO: Self = Self(2);

    #[must_use]
    pub const fn new(batches: u8) -> Option<Self> {
        if batches == 0 {
            None
        } else {
            Some(Self(batches))
        }
    }

    #[must_use]
    pub const fn remaining(self) -> u8 {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FeedbackLifecycle {
    UntilReplaced,
    PlatformBatches(FeedbackBatchLifetime),
    Timed(Duration),
}

impl FeedbackLifecycle {
    pub const NEXT_PLATFORM_BATCH: Self = Self::PlatformBatches(FeedbackBatchLifetime::ONE);
    pub const SHORT: Self = Self::PlatformBatches(FeedbackBatchLifetime::TWO);
    pub const SHORT_DURATION: Duration = Duration::from_secs(5);
    pub const LONG_DURATION: Duration = Duration::from_secs(8);
    pub const TIMED_SHORT: Self = Self::Timed(Self::SHORT_DURATION);
    pub const TIMED_LONG: Self = Self::Timed(Self::LONG_DURATION);

    #[must_use]
    pub const fn for_platform_batches(batches: u8) -> Option<Self> {
        match FeedbackBatchLifetime::new(batches) {
            Some(lifetime) => Some(Self::PlatformBatches(lifetime)),
            None => None,
        }
    }

    #[must_use]
    pub const fn timed(duration: Duration) -> Self {
        Self::Timed(duration)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FeedbackNotice {
    source: FeedbackSource,
    severity: FeedbackSeverity,
    lifecycle: FeedbackLifecycle,
    text: String,
}

impl FeedbackNotice {
    #[must_use]
    pub const fn source(&self) -> FeedbackSource {
        self.source
    }

    #[must_use]
    pub const fn severity(&self) -> FeedbackSeverity {
        self.severity
    }

    #[must_use]
    pub const fn lifecycle(&self) -> FeedbackLifecycle {
        self.lifecycle
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// One feedback authority: long-lived background activity plus at most one
/// point-of-action notice. A notice always wins the visible projection;
/// activity may continue changing underneath it without erasing it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FeedbackState {
    activity: String,
    notice: Option<FeedbackNotice>,
    service: Option<ServiceControlOutcome>,
    startup: Option<StartupControlOutcome>,
    session: Option<SessionControlOutcome>,
}

enum FeedbackEvent {
    SetActivity(String),
    Report(FeedbackNotice),
    ClearNotice,
    AdvancePlatformBatch,
    AdvanceTime(Duration),
    RecordService(ServiceControlOutcome),
    RecordStartup(StartupControlOutcome),
    RecordSession(SessionControlOutcome),
}

impl FeedbackState {
    #[must_use]
    pub fn new(activity: impl Into<String>) -> Self {
        Self {
            activity: activity.into(),
            notice: None,
            service: None,
            startup: None,
            session: None,
        }
    }

    #[must_use]
    pub fn activity(&self) -> &str {
        &self.activity
    }

    #[must_use]
    pub const fn notice(&self) -> Option<&FeedbackNotice> {
        self.notice.as_ref()
    }

    #[must_use]
    pub fn visible_text(&self) -> &str {
        self.notice
            .as_ref()
            .map_or(self.activity.as_str(), FeedbackNotice::text)
    }

    #[must_use]
    pub const fn service(&self) -> Option<&ServiceControlOutcome> {
        self.service.as_ref()
    }

    #[must_use]
    pub const fn startup(&self) -> Option<&StartupControlOutcome> {
        self.startup.as_ref()
    }

    #[must_use]
    pub const fn session(&self) -> Option<&SessionControlOutcome> {
        self.session.as_ref()
    }

    pub fn report_notice(
        &mut self,
        source: FeedbackSource,
        severity: FeedbackSeverity,
        lifecycle: FeedbackLifecycle,
        text: impl Into<String>,
    ) {
        self.reduce(FeedbackEvent::Report(FeedbackNotice {
            source,
            severity,
            lifecycle,
            text: text.into(),
        }));
    }

    pub fn clear_notice(&mut self) {
        self.reduce(FeedbackEvent::ClearNotice);
    }

    pub fn advance_time(&mut self, elapsed: Duration) {
        self.reduce(FeedbackEvent::AdvanceTime(elapsed));
    }

    pub fn record_service(&mut self, outcome: ServiceControlOutcome) {
        self.reduce(FeedbackEvent::RecordService(outcome));
    }

    pub fn record_startup(&mut self, outcome: StartupControlOutcome) {
        self.reduce(FeedbackEvent::RecordStartup(outcome));
    }

    pub fn record_session(&mut self, outcome: SessionControlOutcome) {
        self.reduce(FeedbackEvent::RecordSession(outcome));
    }

    fn reduce(&mut self, event: FeedbackEvent) {
        match event {
            FeedbackEvent::SetActivity(text) => self.activity = text,
            FeedbackEvent::Report(notice) => self.notice = Some(notice),
            FeedbackEvent::ClearNotice => self.notice = None,
            FeedbackEvent::AdvancePlatformBatch => {
                let Some(notice) = self.notice.as_mut() else {
                    return;
                };
                match notice.lifecycle {
                    FeedbackLifecycle::UntilReplaced => {}
                    FeedbackLifecycle::PlatformBatches(lifetime) if lifetime.remaining() == 1 => {
                        self.notice = None;
                    }
                    FeedbackLifecycle::PlatformBatches(lifetime) => {
                        notice.lifecycle = FeedbackLifecycle::PlatformBatches(
                            FeedbackBatchLifetime(lifetime.remaining() - 1),
                        );
                    }
                    FeedbackLifecycle::Timed(_) => {}
                }
            }
            FeedbackEvent::AdvanceTime(elapsed) => {
                let Some(notice) = self.notice.as_mut() else {
                    return;
                };
                if let FeedbackLifecycle::Timed(remaining) = notice.lifecycle {
                    if remaining <= elapsed {
                        self.notice = None;
                    } else {
                        notice.lifecycle = FeedbackLifecycle::Timed(remaining - elapsed);
                    }
                }
            }
            FeedbackEvent::RecordService(outcome) => self.service = Some(outcome),
            FeedbackEvent::RecordStartup(outcome) => self.startup = Some(outcome),
            FeedbackEvent::RecordSession(outcome) => self.session = Some(outcome),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct ShellLifecycleState {
    quit: QuitState,
    feedback: FeedbackState,
}

enum ShellLifecycleEvent {
    RequestQuit(QuitReason),
    Feedback(FeedbackEvent),
}

enum ShellLifecycleEffect {
    None,
    Quit(QuitRequestOutcome),
}

impl ShellLifecycleState {
    pub(super) fn new(activity: impl Into<String>) -> Self {
        Self {
            quit: QuitState::Running,
            feedback: FeedbackState::new(activity),
        }
    }

    fn reduce(&mut self, event: ShellLifecycleEvent) -> ShellLifecycleEffect {
        match event {
            ShellLifecycleEvent::RequestQuit(reason) => {
                let outcome = match self.quit {
                    QuitState::Running => {
                        self.quit = QuitState::Requested(reason);
                        QuitRequestOutcome::Requested(reason)
                    }
                    QuitState::Requested(original) => {
                        QuitRequestOutcome::AlreadyRequested(original)
                    }
                };
                ShellLifecycleEffect::Quit(outcome)
            }
            ShellLifecycleEvent::Feedback(event) => {
                self.feedback.reduce(event);
                ShellLifecycleEffect::None
            }
        }
    }
}

impl ShellApp {
    pub fn request_quit(&mut self, reason: QuitReason) -> QuitRequestOutcome {
        match self
            .lifecycle
            .reduce(ShellLifecycleEvent::RequestQuit(reason))
        {
            ShellLifecycleEffect::Quit(outcome) => outcome,
            // A malformed reducer branch must stay fail-closed and preserve
            // the request reason rather than panic in production.
            ShellLifecycleEffect::None => QuitRequestOutcome::AlreadyRequested(reason),
        }
    }

    #[must_use]
    pub const fn should_quit(&self) -> bool {
        matches!(self.lifecycle.quit, QuitState::Requested(_))
    }

    #[must_use]
    pub const fn quit_reason(&self) -> Option<QuitReason> {
        match self.lifecycle.quit {
            QuitState::Running => None,
            QuitState::Requested(reason) => Some(reason),
        }
    }

    #[must_use]
    pub fn feedback_text(&self) -> &str {
        self.lifecycle.feedback.visible_text()
    }

    #[must_use]
    pub fn feedback_activity(&self) -> &str {
        self.lifecycle.feedback.activity()
    }

    #[must_use]
    pub const fn feedback_notice(&self) -> Option<&FeedbackNotice> {
        self.lifecycle.feedback.notice()
    }

    pub fn set_feedback_activity(&mut self, text: impl Into<String>) {
        let _ = self
            .lifecycle
            .reduce(ShellLifecycleEvent::Feedback(FeedbackEvent::SetActivity(
                text.into(),
            )));
    }

    pub fn report_notice(
        &mut self,
        source: FeedbackSource,
        severity: FeedbackSeverity,
        lifecycle: FeedbackLifecycle,
        text: impl Into<String>,
    ) {
        let _ = self
            .lifecycle
            .reduce(ShellLifecycleEvent::Feedback(FeedbackEvent::Report(
                FeedbackNotice {
                    source,
                    severity,
                    lifecycle,
                    text: text.into(),
                },
            )));
    }

    pub fn clear_feedback_notice(&mut self) {
        let _ = self
            .lifecycle
            .reduce(ShellLifecycleEvent::Feedback(FeedbackEvent::ClearNotice));
    }

    pub fn advance_feedback_time(&mut self, elapsed: Duration) {
        let _ = self
            .lifecycle
            .reduce(ShellLifecycleEvent::Feedback(FeedbackEvent::AdvanceTime(
                elapsed,
            )));
    }

    pub(super) fn advance_feedback_platform_batch(&mut self) {
        let _ = self.lifecycle.reduce(ShellLifecycleEvent::Feedback(
            FeedbackEvent::AdvancePlatformBatch,
        ));
    }
}
