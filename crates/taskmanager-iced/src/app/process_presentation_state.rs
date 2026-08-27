//! Applications/process-details presentation state with no domain authority.

use std::collections::HashSet;

use taskmanager_application::FrozenProcessIdentity;
use taskmanager_shell::SortCol;

use super::DetailsSection;

pub(crate) struct ProcessPresentationState {
    pub(crate) affinity_cpus: Option<HashSet<u32>>,
    pub(crate) expanded_groups: HashSet<String>,
    pub(crate) expanded_tree: HashSet<u32>,
    pub(crate) visual_cursor: usize,
    pub(crate) env_filter: String,
    pub(crate) last_insights_target: Option<FrozenProcessIdentity>,
    pub(crate) hidden_columns: HashSet<SortCol>,
    pub(crate) services_query: String,
    pub(crate) details_section: DetailsSection,
}

impl ProcessPresentationState {
    pub(super) fn new(expanded_groups: HashSet<String>) -> Self {
        Self {
            affinity_cpus: None,
            expanded_groups,
            expanded_tree: HashSet::new(),
            visual_cursor: 0,
            env_filter: String::new(),
            last_insights_target: None,
            hidden_columns: HashSet::new(),
            services_query: String::new(),
            details_section: DetailsSection::default(),
        }
    }
}
