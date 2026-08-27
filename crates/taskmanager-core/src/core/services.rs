//! Platform-neutral service contracts grouped by change domain.

mod control;
mod inventory;
mod log;
mod relation;

pub use control::ServiceAction;
pub use inventory::{ServiceItem, ServiceStatus};
pub use log::{
    ServiceLogAvailability, ServiceLogEntries, ServiceLogEntry, ServiceLogErrorKind,
    ServiceLogFailure, ServiceLogFeed, ServiceLogLevel, ServiceLogLevelFilter, ServiceLogLines,
    ServiceLogProviderState, ServiceLogQuery, ServiceLogSnapshot, ServiceLogState,
    ServiceLogStreamEnd, ServiceLogStreamSnapshot, ServiceLogStreamState, ServiceLogTimeFilter,
};
pub use relation::{ServiceDeps, ServiceRelationEdge, ServiceRelationGraph, ServiceRelationKind};
