//! gpui-based Mission Center UI (replacing the legacy wgpu stack).
//!
//! Reuses the framework-agnostic data layer in [`taskmanager_core::core`] (collector, metrics,
//! process, services, hardware). This module owns only the gpui view code.

pub mod about;
pub mod app_history_view;
pub mod capabilities;
pub mod chrome;
pub mod containers_view;
pub mod cpu_view;
pub mod dashboard;
pub mod elements;
pub mod first_run;
pub mod formatting;
pub mod functional;
pub mod graph;
pub mod help_overlay;
pub(crate) mod history_samples;
pub mod icons;
pub mod list_view;
pub mod perf_views;
pub mod process_insights;
pub mod processes_view;
pub mod root;
pub mod services_view;
pub mod settings_view;
pub mod sidebar;
pub mod startup_view;
pub mod system_about;
pub mod system_health_view;
pub mod system_view;
pub mod theme;
pub mod timeline;
pub mod users_view;

pub(crate) use root::init_demo;
pub use root::{RootView, StartupEnvironment, StartupRuntime, init};
