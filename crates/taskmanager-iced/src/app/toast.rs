//! Renderer-local visibility for one non-modal feedback toast.
//!
//! The shell owns the notice's copy, severity and lifetime. This module only
//! remembers which notice has already been dismissed so a dismissed receipt is
//! not resurrected on the next frame. A replaced notice is admitted as a fresh
//! card; only the shell's typed timed lifecycle expires a notice here.

use std::cell::RefCell;
use std::mem::{Discriminant, discriminant};
use std::time::Duration;
use taskmanager_shell::{FeedbackLifecycle, FeedbackPresentation, ShellApp};

#[derive(Clone, Debug, Eq, PartialEq)]
struct FeedbackIdentity {
    source: String,
    severity: String,
    text: String,
    lifecycle: Discriminant<FeedbackLifecycle>,
}

impl FeedbackIdentity {
    fn from_presentation(presentation: FeedbackPresentation<'_>) -> Self {
        Self {
            source: format!("{:?}", presentation.source()),
            severity: format!("{:?}", presentation.severity()),
            text: presentation.text().to_owned(),
            lifecycle: discriminant(&presentation.lifecycle()),
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct ToastState {
    active: RefCell<Option<ActiveToast>>,
    suppressed: RefCell<Option<FeedbackIdentity>>,
    expired: RefCell<Option<FeedbackIdentity>>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct ActiveToast {
    identity: FeedbackIdentity,
    mounted_at_millis: u64,
    timeout: Option<Duration>,
}

impl ToastState {
    pub(crate) fn sync<'a>(
        &self,
        shell: &'a ShellApp,
        now_millis: u64,
    ) -> Option<FeedbackPresentation<'a>> {
        let Some(presentation) = shell.feedback_presentation() else {
            self.active.replace(None);
            self.suppressed.replace(None);
            self.expired.replace(None);
            return None;
        };
        let identity = FeedbackIdentity::from_presentation(presentation);
        let active = self.active.take();
        let fresh = active
            .as_ref()
            .is_none_or(|active| active.identity != identity);
        if fresh {
            self.suppressed.replace(None);
        }
        let expired = active
            .as_ref()
            .is_some_and(|active| expired(active.timeout, active.mounted_at_millis, now_millis));
        if expired {
            self.active.replace(None);
            self.expired.replace(Some(identity));
            return None;
        }
        if fresh {
            let timeout = match presentation.lifecycle() {
                FeedbackLifecycle::Timed(duration) => Some(duration),
                FeedbackLifecycle::UntilReplaced | FeedbackLifecycle::PlatformBatches(_) => None,
            };
            self.active.replace(Some(ActiveToast {
                identity: identity.clone(),
                mounted_at_millis: now_millis,
                timeout,
            }));
        } else {
            self.active.replace(active);
        }
        if self.suppressed.borrow().as_ref() == Some(&identity) {
            return None;
        }
        if self.expired.borrow().as_ref() == Some(&identity) {
            return None;
        }
        Some(presentation)
    }

    pub(crate) fn dismiss(&self, shell: &ShellApp) {
        *self.suppressed.borrow_mut() = shell
            .feedback_presentation()
            .map(FeedbackIdentity::from_presentation);
    }
}

fn expired(timeout: Option<Duration>, mounted_at_millis: u64, now_millis: u64) -> bool {
    let Some(duration) = timeout else {
        return false;
    };
    now_millis.saturating_sub(mounted_at_millis)
        >= u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
