//! Fair non-blocking drain across primary queues and retained terminals.

use std::sync::Arc;
use std::sync::Mutex;

use crossbeam_channel::{Receiver, TryRecvError};
use taskmanager_application::PlatformEvent;
use taskmanager_platform_contract::{EventEnvelope, EventPort, EventPortError};

use super::catalog::RuntimeCapabilityCatalog;
use super::event_queue::{EventClass, EventQueueState, QueuedEvent};

pub(crate) struct FairEventPort {
    control_rx: Receiver<QueuedEvent>,
    observation_rx: Receiver<QueuedEvent>,
    queues: Arc<EventQueueState>,
    capabilities: Arc<RuntimeCapabilityCatalog>,
    next_partition: Mutex<EventClass>,
    control_primary: Mutex<Option<QueuedEvent>>,
    observation_primary: Mutex<Option<QueuedEvent>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PartitionConnection {
    Connected,
    Disconnected,
}

struct PartitionDrain {
    event: Option<QueuedEvent>,
    connection: PartitionConnection,
}

impl FairEventPort {
    pub(crate) fn new(
        control_rx: Receiver<QueuedEvent>,
        observation_rx: Receiver<QueuedEvent>,
        queues: Arc<EventQueueState>,
        capabilities: Arc<RuntimeCapabilityCatalog>,
    ) -> Self {
        Self {
            control_rx,
            observation_rx,
            queues,
            capabilities,
            next_partition: Mutex::new(EventClass::Control),
            control_primary: Mutex::new(None),
            observation_primary: Mutex::new(None),
        }
    }

    fn try_recv_class(&self, class: EventClass) -> PartitionDrain {
        let receiver = match class {
            EventClass::Control => &self.control_rx,
            EventClass::Observation => &self.observation_rx,
        };
        let primary = match class {
            EventClass::Control => &self.control_primary,
            EventClass::Observation => &self.observation_primary,
        };
        let mut primary = primary
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut primary_disconnected = false;
        if primary.is_none() {
            match receiver.try_recv() {
                Ok(event) => *primary = Some(event),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => primary_disconnected = true,
            }
        }
        let terminal_sequence = self.queues.terminal_front_sequence(class);
        let take_primary = match (
            primary.as_ref().map(|event| event.envelope.sequence),
            terminal_sequence,
        ) {
            (Some(primary), Some(terminal)) => primary <= terminal,
            (Some(_), None) => true,
            (None, _) => false,
        };
        let event = if take_primary {
            let event = primary.take();
            if event.is_some() {
                self.queues.primary_popped(class);
            }
            event
        } else {
            self.queues.pop_terminal(class)
        };
        let connection = if event.is_none() && primary_disconnected && terminal_sequence.is_none() {
            PartitionConnection::Disconnected
        } else {
            PartitionConnection::Connected
        };
        PartitionDrain { event, connection }
    }

    fn deliver(&self, queued: QueuedEvent) -> EventEnvelope<PlatformEvent> {
        if queued.finality.is_terminal() {
            // A terminal becomes queue-visible before its health commit so a
            // failed enqueue can still abort the claimed lifecycle. Join the
            // publisher's transaction before acknowledging: otherwise a fast
            // consumer can retire the request between enqueue and `record`,
            // making the correct health publication look stale.
            let _publication = self.capabilities.terminal_publication_guard();
            self.capabilities.acknowledge_terminal_delivery(
                &queued.envelope.capability,
                queued.envelope.request_id,
            );
        }
        queued.envelope
    }

    fn set_next_partition(&self, next: EventClass) {
        *self
            .next_partition
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = next;
    }

    fn try_recv_ordered(
        &self,
        first: EventClass,
    ) -> Result<Option<EventEnvelope<PlatformEvent>>, EventPortError> {
        let second = first.alternate();
        let first_drain = self.try_recv_class(first);
        if let Some(event) = first_drain.event {
            self.set_next_partition(second);
            return Ok(Some(self.deliver(event)));
        }
        let second_drain = self.try_recv_class(second);
        if let Some(event) = second_drain.event {
            self.set_next_partition(first);
            return Ok(Some(self.deliver(event)));
        }
        if first_drain.connection == PartitionConnection::Disconnected
            && second_drain.connection == PartitionConnection::Disconnected
            && self.queues.terminal_is_empty()
        {
            Err(EventPortError::RuntimeStopped)
        } else {
            Ok(None)
        }
    }
}

impl EventPort for FairEventPort {
    type Event = PlatformEvent;

    fn try_recv(&self) -> Result<Option<EventEnvelope<Self::Event>>, EventPortError> {
        let first = *self
            .next_partition
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        self.try_recv_ordered(first)
    }
}
