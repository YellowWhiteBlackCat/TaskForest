//! test-intent: integration
//!
//! Integration and headless behavior test suite for taskforest-ipc.

#[path = "headless/protocol_tests.rs"]
mod protocol_tests;

#[path = "headless/transport_tests.rs"]
mod transport_tests;

#[path = "headless/service_tests.rs"]
mod service_tests;
