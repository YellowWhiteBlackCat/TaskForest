//! Canonical category-first process-tree projection.

use super::*;

fn collect_tree_members<'a>(node: &ProcessNode<'a>, members: &mut Vec<&'a ProcessItem>) {
    members.push(node.item);
    for child in &node.children {
        collect_tree_members(child, members);
    }
}

fn app_group_from_tree_root(root: &ProcessNode<'_>) -> AppGroup {
    let mut members = Vec::new();
    collect_tree_members(root, &mut members);
    AppGroup {
        name: root
            .item
            .current_application_name()
            .map(str::to_owned)
            .unwrap_or_else(|| root.item.name.clone()),
        main_pid: root.item.pid,
        application_identity: root.item.current_application_identity().cloned(),
        pids: members.iter().map(|process| process.pid).collect(),
        total_cpu_usage: members
            .iter()
            .filter_map(|process| process.current_cpu_percentage())
            .sum(),
        total_memory_bytes: members
            .iter()
            .filter_map(|process| process.current_memory_bytes())
            .sum(),
        process_count: members.len(),
    }
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
    expanded_tree: &HashSet<u32>,
) -> Vec<ProjectedRow> {
    let mut rows = Vec::new();
    for bucket in category_buckets(flat, |process| process_category(process)) {
        let members: Vec<&ProcessItem> = bucket.members().iter().map(|member| **member).collect();
        let key = category_expansion_key(bucket.category());
        let expanded = expanded_groups.contains(&key);
        let mut pids: Vec<u32> = members.iter().map(|process| process.pid).collect();
        pids.sort_unstable();
        let group = AppGroup {
            name: key.clone(),
            main_pid: pids.first().copied().unwrap_or(0),
            application_identity: None,
            total_cpu_usage: bucket.sum_f32(|process| process.current_cpu_percentage()),
            total_memory_bytes: bucket.sum_u64(|process| process.current_memory_bytes()),
            process_count: bucket.member_count(),
            pids,
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
                expanded_groups,
                expanded_tree,
                by_pid,
                flat,
                sort,
            );
        } else {
            flatten_with_parents(&tree, expanded_tree, by_pid, &mut rows, 1, None);
        }
    }
    rows
}

fn push_application_trees(
    rows: &mut Vec<ProjectedRow>,
    tree: &[ProcessNode<'_>],
    expanded_groups: &HashSet<String>,
    expanded_tree: &HashSet<u32>,
    by_pid: &HashMap<u32, usize>,
    flat: &[&ProcessItem],
    sort: (SortCol, SortDir),
) {
    let mut groups: Vec<AppGroup> = tree.iter().map(app_group_from_tree_root).collect();
    sort_groups(&mut groups, by_pid, flat, sort);
    for group in groups {
        let Some(root) = tree.iter().find(|node| node.item.pid == group.main_pid) else {
            continue;
        };
        let expansion_key = format!("app-tree:{}", group.main_pid);
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

fn push_group_header(rows: &mut Vec<ProjectedRow>, group: &AppGroup, input: GroupHeaderInput<'_>) {
    let totals = group_totals(group, input.by_pid, input.flat);
    let (user, status, nice, start_time_secs) = input
        .by_pid
        .get(&group.main_pid)
        .and_then(|index| input.flat.get(*index))
        .map(|process| {
            (
                process.current_user().unwrap_or_default(),
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
        cpu: group.total_cpu_usage,
        pss: totals.pss,
        memory_rss: totals.memory_rss,
        swap: totals.swap,
        disk_read: totals.disk_read,
        disk_write: totals.disk_write,
        threads: totals.threads,
        cpu_time: totals.cpu_time,
        fds: totals.fds,
        user,
        status,
        nice,
        start_time_secs,
        start_clock: taskmanager_shell::presentation::missing_value(),
    });
}
