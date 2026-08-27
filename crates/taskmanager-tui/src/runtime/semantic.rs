//! Toolkit-neutral semantic projection for the terminal frontend.
//!
//! This module consumes the same `taskmanager-ui-contract` snapshot builder
//! as the Iced and GPUI frontends (defect #9: the TUI previously had no
//! semantic channel at all). It deliberately stops at a validated semantic
//! tree: the terminal has no linked native accessibility bridge, so —
//! exactly like Iced's detached projection — a snapshot is not reported as
//! an AT-SPI or screen-reader receipt.
//!
//! The projection carries no terminal geometry. [`SemanticSnapshot`] exposes
//! roles, names, typed values, and percentages only, so widths, row/column
//! counts, and every other Ratatui layout detail are excluded by the
//! contract's types themselves.
//!
//! [`SemanticSnapshot`]: taskmanager_ui_contract::SemanticSnapshot

use taskmanager_application::ProcessItem;
use taskmanager_application::i18n::t;
use taskmanager_assets::product;
use taskmanager_ui_contract::{
    GraphSummary, ModalInput, ProcessRowInput, SemanticSnapshot, SemanticSnapshotBuilder,
};

use crate::TuiApp;

/// Maximum number of process rows published to the semantic tree (GPUI/Iced
/// parity): a screen reader reads the tree top-down, so the bounded prefix
/// of the shell's active ordering is the useful slice.
const MAX_PUBLISHED_ROWS: usize = 64;

impl TuiApp {
    /// Build the current TUI semantic tree without performing terminal I/O.
    ///
    /// The projection is pure state: it reads the shell's refresh counter,
    /// visible process projection, observed system scalars, status line, and
    /// the TUI modal precedence stack, and assembles them through the shared
    /// builder. Unavailable CPU/memory values stay `Unavailable` in the
    /// contract rather than becoming zero; an unobserved CPU scalar omits the
    /// graph instead of fabricating 0%. Returns `None` only if the contract
    /// builder rejects the inputs (surfaced honestly, never panicked).
    #[must_use]
    pub fn semantic_snapshot(&self) -> Option<SemanticSnapshot> {
        let mut builder = SemanticSnapshotBuilder::new(self.shell.projection().refresh_count)
            .application_name(product::NAME)
            .status_announcement(self.semantic_status_text());

        if let Some(current) = self
            .shell
            .projection()
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.cpu.current_global_usage_pct())
            .filter(|value| value.is_finite())
        {
            builder = builder.cpu_graph(GraphSummary {
                current: f64::from(current.clamp(0.0, 100.0)),
                peak: f64::from(current.clamp(0.0, 100.0)),
                maximum: 100.0,
            });
        }

        let memory_total = self
            .shell
            .projection()
            .snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.memory.current_total_bytes());
        // The cursor's process resolves through the category tree (a structural
        // header honestly yields None), and the shell's marked-pid set
        // is the batch-selection tint — both are real selection semantics of
        // the terminal table, so a published row is `selected` when it is the
        // cursor row OR a marked row.
        let cursor_pid = self.selected_detail_process().map(|process| process.pid);
        for process in self
            .visible_processes()
            .into_iter()
            .take(MAX_PUBLISHED_ROWS)
        {
            let name = if process.name.trim().is_empty() {
                String::from("Unnamed process")
            } else {
                process.name.clone()
            };
            builder = builder.process_row(ProcessRowInput {
                id: process.pid.to_string(),
                name,
                cpu_percent: semantic_cpu_percent(process),
                memory_percent: semantic_memory_percent(
                    process.current_memory_bytes(),
                    memory_total,
                ),
                selected: Some(process.pid) == cursor_pid
                    || self.shell.selected_pids.contains(&process.pid),
            });
        }

        if let Some(modal) = self.semantic_active_modal() {
            builder = builder.modal(modal);
        }

        builder.build().ok()
    }

    /// The polite live-region text: the footer status line when the shell set
    /// one this cycle, otherwise the visible row count (Iced parity).
    fn semantic_status_text(&self) -> String {
        if self.shell.feedback_text().trim().is_empty() {
            format!("{} processes visible", self.shell.visible_process_count())
        } else {
            self.shell.feedback_text().to_owned()
        }
    }

    /// The top modal surface on the TUI's precedence stack, or `None` when no
    /// modal-class surface is open. The order mirrors `runtime::modals`
    /// (TUI-local key-trapping modals first, then the shell's confirmation
    /// gates and shared overlays); the shell-owned identifiers match the
    /// Iced/GPUI vocabulary so cross-frontend consumers see one naming
    /// scheme. Search-field activation and the inline details panel are
    /// deliberately excluded: neither traps the keyboard as a modal.
    fn semantic_active_modal(&self) -> Option<ModalInput> {
        match self.input_scope() {
            crate::TuiInputScope::LocalSurface(_) => match self.local_surface()? {
                crate::TuiSurface::CommandPalette(palette) => Some(ModalInput {
                    id: String::from("command-palette"),
                    name: String::from("Command palette"),
                    description: Some(format!("Filter: {}", palette.filter)),
                }),
                crate::TuiSurface::ColumnMenu { .. } => Some(ModalInput {
                    id: String::from("column-visibility-menu"),
                    name: String::from("Column visibility menu"),
                    description: Some(String::from("Toggle the Applications table columns")),
                }),
                crate::TuiSurface::ServiceMenu(menu) => Some(ModalInput {
                    id: String::from("service-action-menu"),
                    name: String::from("Service action menu"),
                    description: Some(format!("Actions for service {}", menu.service.name)),
                }),
                crate::TuiSurface::ProcessMenu(menu) => Some(ModalInput {
                    id: String::from("process-action-menu"),
                    name: String::from("Process action menu"),
                    description: Some(format!(
                        "Actions for process {} ({})",
                        menu.item.pid, menu.item.name
                    )),
                }),
                crate::TuiSurface::BatchMenu(menu) => Some(ModalInput {
                    id: String::from("batch-control-menu"),
                    name: String::from("Batch control menu"),
                    description: Some(format!(
                        "Actions for {} marked processes",
                        menu.marked_count
                    )),
                }),
                crate::TuiSurface::SessionMenu(menu) => Some(ModalInput {
                    id: String::from("session-action-menu"),
                    name: String::from("Session action menu"),
                    description: Some(format!("Actions for session {}", menu.session.user)),
                }),
                crate::TuiSurface::StartupMenu(menu) => Some(ModalInput {
                    id: String::from("startup-action-menu"),
                    name: String::from("Startup action menu"),
                    description: Some(format!(
                        "Enable or disable startup item {}",
                        menu.entry.name
                    )),
                }),
                crate::TuiSurface::Settings => Some(ModalInput {
                    id: String::from("settings"),
                    name: String::from("Settings"),
                    description: Some(String::from("Adjust application preferences")),
                }),
                crate::TuiSurface::About => Some(ModalInput {
                    id: String::from("about"),
                    name: String::from("About"),
                    description: Some(String::from("System and application information")),
                }),
                crate::TuiSurface::Health => Some(ModalInput {
                    id: String::from("health"),
                    name: String::from("System health"),
                    description: Some(String::from("Health and alert overview")),
                }),
                crate::TuiSurface::Containers => Some(ModalInput {
                    id: String::from("containers"),
                    name: String::from("Containers"),
                    description: Some(String::from("Container inventory")),
                }),
            },
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::ProcessProperties,
            ) => self.process_properties().map(|target| ModalInput {
                id: String::from("process-properties-modal"),
                name: String::from("Process properties modal"),
                description: Some(format!(
                    "Properties for {} (PID {})",
                    target.item.name, target.item.pid
                )),
            }),
            crate::TuiInputScope::ServiceLog => {
                self.shell.service_log.as_ref().map(|log| ModalInput {
                    id: String::from("service-log-modal"),
                    name: String::from("Service log modal"),
                    description: Some(format!(
                        "Live logs for {}",
                        log.service_id().map_or("—", |id| id.as_str())
                    )),
                })
            }
            crate::TuiInputScope::Help => Some(ModalInput {
                id: String::from("keyboard-help"),
                name: String::from("Keyboard help"),
                description: Some(String::from("Shared command vocabulary")),
            }),
            crate::TuiInputScope::Suggestions => Some(ModalInput {
                id: String::from("threshold-suggestions"),
                name: String::from(t("alerts.threshold_suggestions")),
                description: Some(String::from("Observed samples only")),
            }),
            crate::TuiInputScope::SharedSurface(
                taskmanager_application::SurfaceKind::Confirmation(_),
            ) => match self.shell.pending_confirmation()? {
                taskmanager_application::PendingConfirmation::EndTask(target) => Some(ModalInput {
                    id: String::from("end-task-confirmation"),
                    name: String::from("End task confirmation"),
                    description: Some(format!(
                        "Confirm the requested action for process {} ({})",
                        target.pid, target.name
                    )),
                }),
                taskmanager_application::PendingConfirmation::ProcessTermination(target) => {
                    Some(ModalInput {
                        id: String::from("process-termination-confirmation"),
                        name: String::from("Process termination confirmation"),
                        description: Some(format!(
                            "Confirm {:?} for process {} ({})",
                            target.action, target.root.pid, target.root.name
                        )),
                    })
                }
                taskmanager_application::PendingConfirmation::ServiceControl(target) => {
                    Some(ModalInput {
                        id: String::from("service-control-confirmation"),
                        name: String::from("Service control confirmation"),
                        description: Some(format!(
                            "Confirm the requested {:?} action for service {}",
                            target.action, target.service_id
                        )),
                    })
                }
                taskmanager_application::PendingConfirmation::ProcessBatch(target) => {
                    Some(ModalInput {
                        id: String::from("batch-action-confirmation"),
                        name: String::from("Batch action confirmation"),
                        description: Some(format!(
                            "Confirm the requested action for {} processes",
                            target.targets.len()
                        )),
                    })
                }
                taskmanager_application::PendingConfirmation::SessionControl(target) => {
                    Some(ModalInput {
                        id: String::from("session-control-confirmation"),
                        name: String::from("Session control confirmation"),
                        description: Some(format!(
                            "Confirm the requested {:?} action for session {}",
                            target.action, target.session.user
                        )),
                    })
                }
                taskmanager_application::PendingConfirmation::StartupControl(target) => {
                    Some(ModalInput {
                        id: String::from("startup-action-confirmation"),
                        name: String::from("Startup action confirmation"),
                        description: Some(format!(
                            "Confirm toggling startup item {}",
                            target.entry.name
                        )),
                    })
                }
                taskmanager_application::PendingConfirmation::SmartSelfTest(target) => {
                    Some(ModalInput {
                        id: String::from("smart-self-test-confirmation"),
                        name: String::from("SMART self-test confirmation"),
                        description: Some(format!(
                            "Confirm {:?} self-test for {}",
                            target.kind, target.display_name
                        )),
                    })
                }
            },
            crate::TuiInputScope::Search
            | crate::TuiInputScope::DetailsPanel
            | crate::TuiInputScope::Content => None,
        }
    }
}

/// Typed CPU percentage for one row, honest about availability (Iced parity):
/// finite values are clamped into the 0..=100 semantic range; everything else
/// stays `None` so the contract renders `Unavailable`, never zero.
fn semantic_cpu_percent(process: &ProcessItem) -> Option<f64> {
    process
        .current_cpu_percentage()
        .filter(|value| value.is_finite())
        .map(|value| f64::from(value.clamp(0.0, 100.0)))
}

/// Typed memory percentage for one row against the system total. `None` when
/// either side is unobserved or the total is not positive, so the contract
/// renders `Unavailable` instead of a fabricated zero.
fn semantic_memory_percent(value: Option<u64>, total: Option<u64>) -> Option<f64> {
    let (value, total) = (value?, total.filter(|total| *total > 0)?);
    // This is a bounded display conversion at the semantic edge; source
    // values remain u64 in the shared process and snapshot contracts.
    let percentage = (value as f64 / total as f64 * 100.0).clamp(0.0, 100.0);
    percentage.is_finite().then_some(percentage)
}
