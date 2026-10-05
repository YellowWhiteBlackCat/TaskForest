//! Identity-bound properties facts, independent of table filtering.

use super::insights::insight_rows;
use super::performance::{PerformanceCurve, performance_curves};
use super::{ProcessPropertiesSection, ProcessPropertiesView};
use taskmanager_application::ProcessInsightFacet;
use taskmanager_application::i18n::t;
use taskmanager_application::process_details_vm::{ProcessDetailsField, process_details_rows};
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_core::core::units::UnitPreferences;
use taskmanager_shell::ShellApp;
use taskmanager_shell::presentation::MISSING_VALUE;

pub(super) fn field_label(field: ProcessDetailsField) -> &'static str {
    match field {
        ProcessDetailsField::Name => t("common.name"),
        ProcessDetailsField::Pid => "PID",
        ProcessDetailsField::ParentPid => t("prop.parent_pid"),
        ProcessDetailsField::AncestorLineage => t("proc.ancestor_lineage"),
        ProcessDetailsField::User => t("common.user"),
        ProcessDetailsField::Status => t("common.status"),
        ProcessDetailsField::Cpu => t("common.cpu"),
        ProcessDetailsField::Memory => t("common.memory"),
        ProcessDetailsField::Pss => t("proc.pss"),
        ProcessDetailsField::Uss => t("proc.uss"),
        ProcessDetailsField::Shared => t("proc.shared"),
        ProcessDetailsField::AnonHugePages => t("proc.anon_huge_pages"),
        ProcessDetailsField::Swap => t("proc.swap"),
        ProcessDetailsField::Threads => t("common.threads"),
        ProcessDetailsField::Fds => t("proc.fds"),
        ProcessDetailsField::Nice => t("proc.nice"),
        ProcessDetailsField::SchedPolicy => t("proc.sched_policy"),
        ProcessDetailsField::OomScore => t("proc.oom_score"),
        ProcessDetailsField::PageFaults => t("proc.page_faults"),
        ProcessDetailsField::StartTime => t("prop.start_time"),
        ProcessDetailsField::CpuTime => t("proc.cpu_time"),
        ProcessDetailsField::DiskReadRate => t("proc.disk_read"),
        ProcessDetailsField::DiskWriteRate => t("proc.disk_write"),
        ProcessDetailsField::NetworkRate => t("common.network"),
        ProcessDetailsField::CancelledWriteBytes => t("proc.cancelled_write"),
        ProcessDetailsField::DiskReadTotal => t("proc.disk_read"),
        ProcessDetailsField::DiskWriteTotal => t("proc.disk_write"),
        ProcessDetailsField::Exe => t("common.executable"),
        ProcessDetailsField::Cmdline => t("prop.command_line"),
    }
}

pub(crate) fn view_model(
    shell: &ShellApp,
    section: ProcessPropertiesSection,
    facet: ProcessInsightFacet,
) -> Option<ProcessPropertiesView> {
    let target = shell.process_properties_target()?;
    let process = target.live_key().and_then(|key| {
        shell
            .projection()
            .processes_slice()
            .iter()
            .find(|process| ProcessLiveKey::from_process(process) == Some(key))
    });
    let mut curves: Vec<PerformanceCurve> = Vec::new();
    let mut rows = Vec::new();
    let mut authorize_network = false;
    if let Some(process) = process {
        let mut current = process.clone();
        current.populate_ancestor_lineage(shell.projection().processes_slice());
        let vm = process_details_rows(&current, &UnitPreferences::default());
        let fields: &[ProcessDetailsField] = match section {
            ProcessPropertiesSection::Overview => &[
                ProcessDetailsField::Pss,
                ProcessDetailsField::Uss,
                ProcessDetailsField::Swap,
                ProcessDetailsField::Memory,
                ProcessDetailsField::Shared,
                ProcessDetailsField::AnonHugePages,
                ProcessDetailsField::Name,
                ProcessDetailsField::Pid,
                ProcessDetailsField::ParentPid,
                ProcessDetailsField::AncestorLineage,
                ProcessDetailsField::User,
                ProcessDetailsField::Status,
                ProcessDetailsField::Cpu,
                ProcessDetailsField::Threads,
                ProcessDetailsField::Fds,
                ProcessDetailsField::Nice,
                ProcessDetailsField::SchedPolicy,
                ProcessDetailsField::OomScore,
                ProcessDetailsField::PageFaults,
                ProcessDetailsField::StartTime,
                ProcessDetailsField::CpuTime,
                ProcessDetailsField::NetworkRate,
                ProcessDetailsField::CancelledWriteBytes,
                ProcessDetailsField::DiskReadTotal,
                ProcessDetailsField::DiskWriteTotal,
            ],
            ProcessPropertiesSection::Command => &[
                ProcessDetailsField::Name,
                ProcessDetailsField::Exe,
                ProcessDetailsField::Cmdline,
            ],
            ProcessPropertiesSection::Performance | ProcessPropertiesSection::Insights => &[],
        };
        for field in fields {
            if let Some(row) = vm.iter().find(|row| row.field == *field) {
                rows.push((
                    field_label(*field).to_owned(),
                    row.value.text_or(MISSING_VALUE).to_owned(),
                ));
            }
        }
        if section == ProcessPropertiesSection::Performance {
            curves = performance_curves(&current, &vm);
        }
        if section == ProcessPropertiesSection::Insights {
            let projection = shell
                .projection()
                .process_insights
                .as_ref()
                .filter(|projection| projection.target == *target);
            (rows, authorize_network) = insight_rows(projection, facet);
        }
    } else {
        rows = vec![
            (t("common.name").to_owned(), target.name.clone()),
            ("PID".to_owned(), target.pid.to_string()),
            (
                t("common.status").to_owned(),
                t("feedback.process_gone").to_owned(),
            ),
        ];
    }
    Some(ProcessPropertiesView {
        target: target.clone(),
        section,
        facet,
        rows,
        curves,
        authorize_network,
        live: process.is_some(),
    })
}
