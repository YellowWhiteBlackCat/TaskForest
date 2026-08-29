//! Bounded event-queue accounting and non-blocking terminal retention.

use std::collections::VecDeque;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use taskmanager_application::PlatformEvent;
use taskmanager_platform_contract::{EventEnvelope, EventSequence};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EventClass {
    Control,
    Observation,
}

impl EventClass {
    /// The other batch partition in the fair two-partition drain order.
    pub(super) const fn alternate(self) -> Self {
        match self {
            Self::Control => Self::Observation,
            Self::Observation => Self::Control,
        }
    }
}

/// Delivery lifecycle carried with a queued platform event.
///
/// Progress may be coalesced when its primary partition is full. A terminal
/// publication has already claimed the request owner, so it must either enter
/// the primary queue or the bounded terminal mailbox.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum EventFinality {
    Progress,
    Terminal,
}

impl EventFinality {
    pub(super) const fn is_terminal(self) -> bool {
        matches!(self, Self::Terminal)
    }
}

pub(crate) struct QueuedEvent {
    pub(super) envelope: EventEnvelope<PlatformEvent>,
    pub(super) finality: EventFinality,
}

impl std::ops::Deref for QueuedEvent {
    type Target = EventEnvelope<PlatformEvent>;

    fn deref(&self) -> &Self::Target {
        &self.envelope
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct EventQueuePressureSnapshot {
    pub(super) control_pending: usize,
    pub(super) control_high_water: usize,
    pub(super) observation_pending: usize,
    pub(super) observation_high_water: usize,
    pub(super) terminal_mailbox_pending: usize,
    pub(super) terminal_mailbox_high_water: usize,
}

#[derive(Default)]
struct QueuePressure {
    control_pending: AtomicUsize,
    control_high_water: AtomicUsize,
    observation_pending: AtomicUsize,
    observation_high_water: AtomicUsize,
    terminal_mailbox_pending: AtomicUsize,
    terminal_mailbox_high_water: AtomicUsize,
}

impl QueuePressure {
    fn increment(pending: &AtomicUsize, high_water: &AtomicUsize) {
        let current = pending.fetch_add(1, Ordering::AcqRel).saturating_add(1);
        high_water.fetch_max(current, Ordering::AcqRel);
    }

    fn decrement(pending: &AtomicUsize) {
        let _ = pending.fetch_update(Ordering::AcqRel, Ordering::Acquire, |current| {
            Some(current.saturating_sub(1))
        });
    }

    fn primary_pushed(&self, class: EventClass) {
        match class {
            EventClass::Control => Self::increment(&self.control_pending, &self.control_high_water),
            EventClass::Observation => {
                Self::increment(&self.observation_pending, &self.observation_high_water)
            }
        }
    }

    fn primary_popped(&self, class: EventClass) {
        match class {
            EventClass::Control => Self::decrement(&self.control_pending),
            EventClass::Observation => Self::decrement(&self.observation_pending),
        }
    }

    fn terminal_pushed(&self) {
        Self::increment(
            &self.terminal_mailbox_pending,
            &self.terminal_mailbox_high_water,
        );
    }

    fn terminal_popped(&self) {
        Self::decrement(&self.terminal_mailbox_pending);
    }

    fn snapshot(&self) -> EventQueuePressureSnapshot {
        EventQueuePressureSnapshot {
            control_pending: self.control_pending.load(Ordering::Acquire),
            control_high_water: self.control_high_water.load(Ordering::Acquire),
            observation_pending: self.observation_pending.load(Ordering::Acquire),
            observation_high_water: self.observation_high_water.load(Ordering::Acquire),
            terminal_mailbox_pending: self.terminal_mailbox_pending.load(Ordering::Acquire),
            terminal_mailbox_high_water: self.terminal_mailbox_high_water.load(Ordering::Acquire),
        }
    }
}

#[derive(Default)]
struct TerminalMailboxState {
    control: VecDeque<QueuedEvent>,
    observation: VecDeque<QueuedEvent>,
}

pub(crate) struct EventQueueState {
    terminal_capacity: usize,
    terminal: Mutex<TerminalMailboxState>,
    pressure: QueuePressure,
}

impl EventQueueState {
    pub(crate) fn new(terminal_capacity: usize) -> Self {
        Self {
            terminal_capacity,
            terminal: Mutex::new(TerminalMailboxState::default()),
            pressure: QueuePressure::default(),
        }
    }

    pub(super) fn primary_pushed(&self, class: EventClass) {
        self.pressure.primary_pushed(class);
    }

    pub(super) fn primary_popped(&self, class: EventClass) {
        self.pressure.primary_popped(class);
    }

    pub(super) fn retain_terminal(&self, class: EventClass, event: QueuedEvent) -> bool {
        let mut terminal = self
            .terminal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if terminal
            .control
            .len()
            .saturating_add(terminal.observation.len())
            >= self.terminal_capacity
        {
            return false;
        }
        match class {
            EventClass::Control => terminal.control.push_back(event),
            EventClass::Observation => terminal.observation.push_back(event),
        }
        self.pressure.terminal_pushed();
        drop(terminal);
        true
    }

    pub(super) fn pop_terminal(&self, class: EventClass) -> Option<QueuedEvent> {
        let event = {
            let mut terminal = self
                .terminal
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            match class {
                EventClass::Control => terminal.control.pop_front(),
                EventClass::Observation => terminal.observation.pop_front(),
            }
        };
        if event.is_some() {
            self.pressure.terminal_popped();
        }
        event
    }

    pub(super) fn terminal_front_sequence(&self, class: EventClass) -> Option<EventSequence> {
        let terminal = self
            .terminal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        match class {
            EventClass::Control => terminal.control.front(),
            EventClass::Observation => terminal.observation.front(),
        }
        .map(|event| event.envelope.sequence)
    }

    pub(super) fn terminal_is_empty(&self) -> bool {
        let terminal = self
            .terminal
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        terminal.control.is_empty() && terminal.observation.is_empty()
    }

    pub(super) fn pressure_snapshot(&self) -> EventQueuePressureSnapshot {
        self.pressure.snapshot()
    }
}
