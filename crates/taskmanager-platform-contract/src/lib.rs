//! Platform-neutral contracts shared by application use cases and native adapters.
//!
//! This crate deliberately contains no domain payloads and no operating-system
//! implementation. It defines runtime capabilities, request correlation,
//! provider failures, composite snapshots, and non-blocking request/event ports.
//! Stable identity, completed-operation failure, and source-truth primitives are
//! owned by `taskmanager-core` and re-exported here for port consumers.

#![forbid(unsafe_code)]

mod capability;
mod envelope;
mod failure;
mod instance;
mod port;
mod scheduler;
mod source;
mod tray;

pub use capability::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilityRequest, CapabilitySnapshot,
    CapabilityStatus, MAX_REQUEST_SCOPE_BYTES, RequestScope, RequestTracking, RequestTrackingError,
    SidebandPolicy,
};
pub use envelope::{EventEnvelope, EventSequence, RequestEnvelope, RequestId, RequestIdGenerator};
pub use failure::{
    EventPortError, OperationFailure, ProviderFailure, RetryDisposition, SubmissionError,
    SubmissionErrorKind, TrayFailure,
};
pub use instance::{InstanceEvent, InstanceFailure, InstanceGuard, InstanceRole};
pub use port::{EventPort, RequestPort};
pub use scheduler::{
    CapabilityRecoveryOutcome, CapabilityRecoveryTrigger, CapabilityScheduler,
    DomainSchedulingSnapshot, EventQueueSchedulingSnapshot, MAX_PROVIDER_PANIC_MESSAGE_CHARS,
    MAX_PROVIDER_PANIC_NOTES, MAX_RECENT_SCHEDULING_STALLS, ProviderPanicNote,
    RuntimeSchedulingSnapshot, SchedulingAdmissionSnapshot, SchedulingBudgetSnapshot,
    SchedulingDomain, SchedulingScope, SchedulingStall,
};
pub use source::{
    CompositeSourceSnapshot, DeviceDiscovery, DeviceSourceSnapshot, PartialSourceSnapshot,
};
pub use taskmanager_core::{
    DeviceGeneration, DeviceId, FailureKind, ProviderId, SourceOutcome, SourceStatus,
};
pub use tray::TrayController;
