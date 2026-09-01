#![allow(linker_messages)]

#[path = "logic/capture_evidence_test.rs"]
mod capture_evidence_test;

#[path = "logic/frontend_submission_ownership_test.rs"]
mod frontend_submission_ownership_test;

#[path = "logic/frontend_dependency_closure_test.rs"]
mod frontend_dependency_closure_test;

#[path = "logic/iced_dependency_coherence_test.rs"]
mod iced_dependency_coherence_test;

#[path = "logic/process_insights_parity_ledger_test.rs"]
mod process_insights_parity_ledger_test;

#[path = "logic/gpui_interaction_matrix_test.rs"]
mod gpui_interaction_matrix_test;

#[path = "logic/accessibility_architecture_test.rs"]
mod accessibility_architecture_test;

#[path = "logic/ecs_application_bridge_test.rs"]
mod ecs_application_bridge_test;

#[path = "logic/hardware_feature_matrix_architecture_test.rs"]
mod hardware_feature_matrix_architecture_test;

#[cfg(target_os = "linux")]
#[path = "logic/metrics_test.rs"]
mod metrics_test;

// The whole module exercises the Linux provider's pure disk-name/init parsers
// (`taskmanager-platform-linux` is a Linux-only dev-dependency), so it compiles
// only there; the Windows/macOS surfaces prove their own helpers in-crate.
#[cfg(target_os = "linux")]
#[path = "logic/hardware_test.rs"]
mod hardware_test;

#[path = "logic/native_os_adapter_test.rs"]
mod native_os_adapter_test;

#[path = "logic/network_hot_path_allocation_gate.rs"]
mod network_hot_path_allocation_gate;

#[path = "logic/process_test.rs"]
mod process_test;

#[path = "logic/live_smoke_test.rs"]
mod live_smoke_test;

#[path = "logic/process_telemetry_test.rs"]
mod process_telemetry_test;

#[path = "logic/quality_gate_test.rs"]
mod quality_gate_test;

#[path = "logic/workspace_architecture_test.rs"]
mod workspace_architecture_test;

#[path = "logic/ui_component_boundary.rs"]
mod ui_component_boundary;

#[path = "logic/theme_neutrality_test.rs"]
mod theme_neutrality_test;

#[path = "logic/panic_surface_test.rs"]
mod panic_surface_test;

// Linux init systems only: the module drives the real systemd/OpenRC scan
// path from the Linux-only dev-dependency.
#[cfg(target_os = "linux")]
#[path = "logic/services_test.rs"]
mod services_test;

#[path = "logic/hardware_data.rs"]
mod hardware_data;

#[path = "logic/reexport_alias_gate_test.rs"]
mod reexport_alias_gate_test;

#[path = "logic/renderer_fold_boundary.rs"]
mod renderer_fold_boundary;

#[path = "logic/control_vocabulary_boundary.rs"]
mod control_vocabulary_boundary;

#[path = "logic/control_semantic_parity.rs"]
mod control_semantic_parity;

#[path = "logic/shared_fold_write_boundary.rs"]
mod shared_fold_write_boundary;

#[path = "common/test_support.rs"]
mod test_support;
