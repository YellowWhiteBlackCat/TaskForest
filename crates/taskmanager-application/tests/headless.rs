//! Application integration tests that consume application-owned public APIs
//! plus direct owner paths for core facts and platform contracts.

#[path = "headless/application_interaction_tests.rs"]
mod application_interaction_tests;

#[path = "headless/application_config_runtime_tests.rs"]
mod application_config_runtime_tests;

#[path = "headless/application_history_replay_tests.rs"]
mod application_history_replay_tests;

#[path = "headless/application_boot_baseline_tests.rs"]
mod application_boot_baseline_tests;

#[path = "headless/application_service_lifecycle_tests.rs"]
mod application_service_lifecycle_tests;

#[path = "headless/application_request_session_tests.rs"]
mod application_request_session_tests;
