//! Canonical category-first process-tree projection.

use super::*;
use taskmanager_core::core::process::ProcessLiveKey;
use taskmanager_shell::presentation::missing_value;

fn collect_tree_members<'a>(node: &ProcessNode<'a>, members: &mut Vec<&'a ProcessItem>) {
    members.push(node.item);
    for child in &node.children {
        collect_tree_members(child, members);
    }
}

fn app_group_from_tree_root<'a>(
    root: &ProcessNode<'a>,
    observed_at_ms: u64,
) -> Option<GroupProjection> {
    let mut members = Vec::new();
    collect_tree_members(root, &mut members);
    let metrics = aggregate_group_metrics(&members, observed_at_ms)?;
    Some(GroupProjection {
        name: root
            .item
            .current_application_name()
            .map(str::to_owned)
            .unwrap_or_else(|| root.item.name.clone()),
        main_pid: root.item.pid,
        process_count: members.len(),
        metrics,
    })
}

fn category_label(category: ProcessCategory) -> &'static str {
    match category {
        ProcessCategory::Application => t("proc.category_apps"),
        ProcessCategory::Background => t("proc.category_background"),
        ProcessCategory::Uncategorized => t("proc.category_uncategorized"),
    }
}

pub(super) fn category_rows(
    flat: &[&ProcessItem],
    by_pid: &HashMap<u32, usize>,
    sort: (SortCol, SortDir),
    expanded_groups: &HashSet<String>,
    expanded_tree: &HashSet<ProcessLiveKey>,
    observed_at_ms: u64,
) -> Vec<ProjectedRow> {
    let mut rows = Vec::new();
    for bucket in category_buckets(flat, |process| process_category(process)) {
        let members: Vec<&ProcessItem> = bucket.members().iter().map(|member| **member).collect();
        let key = category_expansion_key(bucket.category());
        let expanded = expanded_groups.contains(&key);
        let mut pids: Vec<u32> = members.iter().map(|process| process.pid).collect();
        pids.sort_unstable();
        let Some(main_pid) = pids.first().copied() else {
            continue;
        };
        let Some(metrics) = aggregate_group_metrics(&members, observed_at_ms) else {
            continue;
        };
        let group = GroupProjection {
            name: key.clone(),
            main_pid,
            process_count: bucket.member_count(),
            metrics,
        };
        push_group_header(
            &mut rows,
            &group,
            GroupHeaderInput {
                row_key: None,
                name: category_label(bucket.category()).to_owned(),
                expansion_key: key,
                expanded,
                by_pid,
                flat,
            },
        );
        if !expanded {
            continue;
        }

        let mut tree = build_process_tree(&members);
        sort_tree(&mut tree, sort);
        if bucket.category() == ProcessCategory::Application {
            push_application_trees(
                &mut rows,
                &tree,
                ApplicationTreeContext {
                    expanded_groups,
                    expanded_tree,
                    by_pid,
                    flat,
                    sort,
                    observed_at_ms,
                },
            );
        } else {
            flatten_with_parents(&tree, expanded_tree, by_pid, &mut rows, 1, None);
        }
    }
    rows
}

struct ApplicationTreeContext<'a> {
    expanded_groups: &'a HashSet<String>,
    expanded_tree: &'a HashSet<ProcessLiveKey>,
    by_pid: &'a HashMap<u32, usize>,
    flat: &'a [&'a ProcessItem],
    sort: (SortCol, SortDir),
    observed_at_ms: u64,
}

fn push_application_trees(
    rows: &mut Vec<ProjectedRow>,
    tree: &[ProcessNode<'_>],
    context: ApplicationTreeContext<'_>,
) {
    let ApplicationTreeContext {
        expanded_groups,
        expanded_tree,
        by_pid,
        flat,
        sort,
        observed_at_ms,
    } = context;
    let mut groups: Vec<GroupProjection> = tree
        .iter()
        .filter_map(|root| app_group_from_tree_root(root, observed_at_ms))
        .collect();
    sort_groups(&mut groups, sort);
    for group in groups {
        let Some(root) = tree.iter().find(|node| node.item.pid == group.main_pid) else {
            continue;
        };
        let Some(root_identity) = ProcessLiveKey::from_process(root.item) else {
            continue;
        };
        let expansion_key = format!("app-tree:{}", root_identity.stable_key());
        let expanded = expanded_groups.contains(&expansion_key);
        push_group_header(
            rows,
            &group,
            GroupHeaderInput {
                row_key: ProcessRowId::application_of(root.item),
                name: group.name.clone(),
                expansion_key,
                expanded,
                by_pid,
                flat,
            },
        );
        if expanded {
            flatten_with_parents(
                std::slice::from_ref(root),
                expanded_tree,
                by_pid,
                rows,
                2,
                None,
            );
        }
    }
}

struct GroupHeaderInput<'a> {
    row_key: Option<ProcessRowId>,
    name: String,
    expansion_key: String,
    expanded: bool,
    by_pid: &'a HashMap<u32, usize>,
    flat: &'a [&'a ProcessItem],
}

fn push_group_header(
    rows: &mut Vec<ProjectedRow>,
    group: &GroupProjection,
    input: GroupHeaderInput<'_>,
) {
    let (user, status, nice, start_time_secs) = input
        .by_pid
        .get(&group.main_pid)
        .and_then(|index| input.flat.get(*index))
        .map(|process| {
            (
                process.current_user().unwrap_or_else(missing_value),
                process.status.clone(),
                process.current_nice(),
                process.current_start_time_secs(),
            )
        })
        .unwrap_or_default();
    rows.push(ProjectedRow::GroupHeader {
        flat_index: input.by_pid.get(&group.main_pid).copied().unwrap_or(0),
        main_pid: group.main_pid,
        row_key: input.row_key,
        name: input.name,
        expansion_key: input.expansion_key,
        member_count: group.process_count,
        expanded: input.expanded,
        metrics: Box::new(group.metrics.clone()),
        user,
        status,
        nice,
        start_time_secs,
        start_clock: taskmanager_shell::presentation::missing_value(),
    });
}
