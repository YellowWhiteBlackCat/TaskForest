//! Platform-neutral contracts shared by application use cases and native adapters.
//!
//! This crate deliberately contains no domain payloads and no operating-system
//! implementation. It defines runtime capabilities, request correlation,
//! provider failures, composite snapshots, and non-blocking request/event ports.
//! Stable identity, completed-operation failure, and source-truth primitives are
//! owned by `taskmanager-core`; port consumers import them from that owner.

#![forbid(unsafe_code)]

mod capability;
mod envelope;
mod failure;
mod instance;
mod platform;
mod port;
mod scheduler;
mod source;
mod tray;
mod window_capture;

pub use capability::{
    CapabilityCatalog, CapabilityDescriptor, CapabilityId, CapabilityRequest, CapabilitySnapshot,
    CapabilityStatus, MAX_REQUEST_SCOPE_BYTES, PUBLIC_CAPABILITY_DEGRADATION_WORDS,
    PUBLIC_FAILURE_AXIS_PERMISSION_WORD, RequestScope, RequestTracking, RequestTrackingError,
    SidebandPolicy,
};
pub use envelope::{EventEnvelope, EventSequence, RequestEnvelope, RequestId, RequestIdGenerator};
pub use failure::{
    EventPortError, OperationFailure, ProviderFailure, RetryDisposition, SubmissionError,
    SubmissionErrorKind, TrayFailure,
};
pub use instance::{InstanceEvent, InstanceFailure, InstanceGuard, InstanceRole};
pub use platform::PlatformAxis;
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
pub use tray::TrayController;
pub use window_capture::{
    InProcessCaptureFn, MAX_WINDOW_CAPTURE_FAILURE_CHARS, NativeWindowCapture,
    WindowCaptureBackend, WindowCaptureFailure, WindowCaptureFailureKind, WindowCaptureReceipt,
};
