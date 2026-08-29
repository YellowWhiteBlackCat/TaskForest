//! Private ownership for GPUI renderer projection caches.
//!
//! Render entry points receive immutable `Rc` snapshots. Interior mutability
//! stays inside this component, and no caller can retain a `RefCell` guard
//! across another projection build.

use std::cell::RefCell;
use std::rc::Rc;

use taskmanager_core::core::services::ServiceItem;
use taskmanager_core::core::session::SessionItem;
use taskmanager_core::core::startup::StartupEntry;

use taskmanager_shell::{InfoSortCol, SortDir};

use super::tooltip::ProcessHistories;
use crate::gpui_app::app_history_view::AppHistoryRow;
use crate::gpui_app::processes_view::rows::ProjectionCache;
use crate::gpui_app::services_view::ServiceFilter;
use crate::gpui_app::startup_view::StartupFilter;
use taskmanager_core::core::process::ProcessItem;

struct ServicesEntry {
    generation: u64,
    filter: ServiceFilter,
    query: String,
    sort: Option<(InfoSortCol, SortDir)>,
    rows: Rc<Vec<ServiceItem>>,
}

struct StartupEntryCache {
    generation: u64,
    filter: StartupFilter,
    query: String,
    sort: Option<(InfoSortCol, SortDir)>,
    rows: Rc<Vec<StartupEntry>>,
}

struct SessionsEntry {
    generation: u64,
    sort: Option<(InfoSortCol, SortDir)>,
    rows: Rc<Vec<SessionItem>>,
}

struct AppHistoryEntry {
    source: std::sync::Arc<[taskmanager_application::ApplicationHistoryRow]>,
    rows: Rc<Vec<AppHistoryRow>>,
}

struct ProcessDetailsEntry {
    snapshot: Rc<Vec<ProcessItem>>,
    pid: u32,
    item: Rc<ProcessItem>,
    histories: Rc<ProcessHistories>,
}

#[derive(Default)]
pub(super) struct GpuiProjectionCaches {
    processes: Option<ProjectionCache>,
    services: RefCell<Option<ServicesEntry>>,
    startup: RefCell<Option<StartupEntryCache>>,
    sessions: RefCell<Option<SessionsEntry>>,
    app_history: RefCell<Option<AppHistoryEntry>>,
    process_details: RefCell<Option<ProcessDetailsEntry>>,
}

impl GpuiProjectionCaches {
    pub(super) const fn processes(&self) -> Option<&ProjectionCache> {
        self.processes.as_ref()
    }

    pub(super) fn replace_processes(&mut self, cache: ProjectionCache) {
        self.processes = Some(cache);
    }

    pub(super) fn application_count(&self) -> usize {
        self.processes
            .as_ref()
            .map_or(0, |cache| cache.application_count)
    }

    pub(super) fn services(
        &self,
        generation: u64,
        filter: ServiceFilter,
        query: String,
        sort: Option<(InfoSortCol, SortDir)>,
        build: impl FnOnce() -> Vec<ServiceItem>,
    ) -> Rc<Vec<ServiceItem>> {
        {
            let cache = self.services.borrow();
            if let Some(entry) = cache.as_ref()
                && entry.generation == generation
                && entry.filter == filter
                && entry.query == query
                && entry.sort == sort
            {
                return Rc::clone(&entry.rows);
            }
        }
        let rows = Rc::new(build());
        *self.services.borrow_mut() = Some(ServicesEntry {
            generation,
            filter,
            query,
            sort,
            rows: Rc::clone(&rows),
        });
        rows
    }

    pub(super) fn startup(
        &self,
        generation: u64,
        filter: StartupFilter,
        query: String,
        sort: Option<(InfoSortCol, SortDir)>,
        build: impl FnOnce() -> Vec<StartupEntry>,
    ) -> Rc<Vec<StartupEntry>> {
        {
            let cache = self.startup.borrow();
            if let Some(entry) = cache.as_ref()
                && entry.generation == generation
                && entry.filter == filter
                && entry.query == query
                && entry.sort == sort
            {
                return Rc::clone(&entry.rows);
            }
        }
        let rows = Rc::new(build());
        *self.startup.borrow_mut() = Some(StartupEntryCache {
            generation,
            filter,
            query,
            sort,
            rows: Rc::clone(&rows),
        });
        rows
    }

    pub(super) fn sessions(
        &self,
        generation: u64,
        sort: Option<(InfoSortCol, SortDir)>,
        build: impl FnOnce() -> Vec<SessionItem>,
    ) -> Rc<Vec<SessionItem>> {
        {
            let cache = self.sessions.borrow();
            if let Some(entry) = cache.as_ref()
                && entry.generation == generation
                && entry.sort == sort
            {
                return Rc::clone(&entry.rows);
            }
        }
        let rows = Rc::new(build());
        *self.sessions.borrow_mut() = Some(SessionsEntry {
            generation,
            sort,
            rows: Rc::clone(&rows),
        });
        rows
    }

    pub(super) fn app_history(
        &self,
        source: &std::sync::Arc<[taskmanager_application::ApplicationHistoryRow]>,
        build: impl FnOnce() -> Vec<AppHistoryRow>,
    ) -> Rc<Vec<AppHistoryRow>> {
        {
            let cache = self.app_history.borrow();
            if let Some(entry) = cache.as_ref()
                && std::sync::Arc::ptr_eq(&entry.source, source)
            {
                return Rc::clone(&entry.rows);
            }
        }
        let rows = Rc::new(build());
        *self.app_history.borrow_mut() = Some(AppHistoryEntry {
            source: std::sync::Arc::clone(source),
            rows: Rc::clone(&rows),
        });
        rows
    }

    pub(super) fn process_details(
        &self,
        snapshot: &Rc<Vec<ProcessItem>>,
        pid: u32,
    ) -> Option<(Rc<ProcessItem>, Rc<ProcessHistories>)> {
        let cache = self.process_details.borrow();
        let entry = cache.as_ref()?;
        (entry.pid == pid && Rc::ptr_eq(&entry.snapshot, snapshot))
            .then(|| (Rc::clone(&entry.item), Rc::clone(&entry.histories)))
    }

    pub(super) fn replace_process_details(
        &self,
        snapshot: Rc<Vec<ProcessItem>>,
        pid: u32,
        item: Rc<ProcessItem>,
        histories: Rc<ProcessHistories>,
    ) {
        *self.process_details.borrow_mut() = Some(ProcessDetailsEntry {
            snapshot,
            pid,
            item,
            histories,
        });
    }
}
