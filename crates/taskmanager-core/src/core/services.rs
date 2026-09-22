//! Platform-neutral service contracts grouped by change domain.

mod control;
mod inventory;
mod log;
mod relation;

pub use control::ServiceAction;
pub use inventory::{
    ServiceDiagnostics, ServiceFailureCause, ServiceItem, ServiceOomCause, ServiceStatus,
    service_exit_status_label,
};
pub use log::{
    ServiceLogAvailability, ServiceLogEntries, ServiceLogEntry, ServiceLogErrorKind,
    ServiceLogFailure, ServiceLogFeed, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogLines,
    ServiceLogProviderState, ServiceLogQuery, ServiceLogSnapshot, ServiceLogState,
    ServiceLogStreamEnd, ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
};
pub use relation::{
    DirectedServiceEdge, ServiceCycle, ServiceDeps, ServiceEdgeCategory, ServiceRelationEdge,
    ServiceRelationGraph, ServiceRelationKind, detect_directed_cycles, detect_ordering_cycles,
    detect_requirement_cycles, is_ordering_acyclic, is_requirement_acyclic,
};
