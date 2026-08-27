//! Frontend-local bounded ring buffer for the process-details Performance tab
//! (ADR-028): CPU%, memory bytes, and disk read/write rates for ONE tracked
//! pid.
//!
//! The former system-wide `PerfHistory` headline ring was retired (G-02): the
//! Performance page's CPU%/memory% chart and summary lines now read the
//! shell's shared `LiveGraphHistory` series directly (`MetricSeries::CpuUsagePercent`
//! / `MemoryUsagePercent`), sized by the persisted graph-data-points preference
//! through [`taskmanager_shell::ShellApp::set_history_capacity`] — one
//! sanctioned store per series instead of the per-tick double collection.
//!
//! This per-process window stays renderer-local because the shell owns no
//! per-process series: it is seeded from the provider-pre-populated
//! `ProcessItem` history windows the moment the details overlay opens (G-14,
//! [`ProcessPerfHistory::seed_from_provider`]) and then extended by the live
//! ~1 Hz sampling path while the overlay stays open. On mac/win, where the
//! provider leaves those windows empty, the live sampling alone fills the
//! ring — the unchanged fallback.

use std::collections::VecDeque;
use std::rc::Rc;

/// The upper bound for a persisted graph-data-points window (GPUI parity:
/// `MAX_GRAPH_DATA_POINTS = 600`).
pub const MAX_HISTORY_CAPACITY: usize = 600;

/// The newest `capacity` samples of a provider history window (the trailing
/// slice — a window longer than the ring keeps its most recent samples only).
fn newest_tail(window: &[f32], capacity: usize) -> &[f32] {
    let start = window.len().saturating_sub(capacity);
    &window[start..]
}

/// The per-process Performance-tab window (GPUI's `details_performance`
/// parity): CPU%, memory bytes, and disk read/write rates for ONE tracked pid.
/// Pushing a different pid clears the window so the sparklines never blend
/// two processes; each series pushes independently and rejects non-finite
/// values.
#[derive(Clone, Debug)]
pub struct ProcessPerfHistory {
    pid: u32,
    cpu: VecDeque<f32>,
    memory: VecDeque<f32>,
    disk_read: VecDeque<f32>,
    disk_write: VecDeque<f32>,
    capacity: usize,
    revision: u64,
}

/// Contiguous, shared series for one process-properties render. The cache in
/// `IcedApp` returns the same `Rc` handles until the ring revision advances.
#[derive(Clone, Debug)]
pub(crate) struct ProcessPerfHistorySnapshot {
    pub(crate) cpu: Rc<[f32]>,
    pub(crate) memory: Rc<[f32]>,
    pub(crate) disk_read: Rc<[f32]>,
    pub(crate) disk_write: Rc<[f32]>,
}

/// Renderer-local cache entry keyed by the tracked pid and ring revision.
#[derive(Clone, Debug)]
pub(crate) struct ProcessPerfHistoryCache {
    pub(crate) pid: u32,
    pub(crate) revision: u64,
    pub(crate) snapshot: ProcessPerfHistorySnapshot,
}

impl ProcessPerfHistory {
    /// Build the window with the persisted graph-data-points capacity.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        Self {
            pid: 0,
            cpu: VecDeque::new(),
            memory: VecDeque::new(),
            disk_read: VecDeque::new(),
            disk_write: VecDeque::new(),
            capacity: capacity.clamp(2, MAX_HISTORY_CAPACITY),
            revision: 0,
        }
    }

    /// Record one point-in-time sample for `pid`. A pid change resets all
    /// series first (the tracked process identity is authoritative).
    pub fn push(
        &mut self,
        pid: u32,
        cpu: Option<f32>,
        memory: Option<u64>,
        disk_read: Option<u64>,
        disk_write: Option<u64>,
    ) {
        if pid != 0 && self.pid != pid {
            self.reset(pid);
        }
        let mut changed = false;
        if let Some(value) = cpu {
            changed |= Self::push_one(&mut self.cpu, value, self.capacity);
        }
        if let Some(value) = memory {
            changed |= Self::push_one(&mut self.memory, value as f32, self.capacity);
        }
        if let Some(value) = disk_read {
            changed |= Self::push_one(&mut self.disk_read, value as f32, self.capacity);
        }
        if let Some(value) = disk_write {
            changed |= Self::push_one(&mut self.disk_write, value as f32, self.capacity);
        }
        if changed {
            self.revision = self.revision.wrapping_add(1);
        }
    }

    fn push_one(buf: &mut VecDeque<f32>, value: f32, capacity: usize) -> bool {
        if value.is_finite() {
            if buf.len() >= capacity {
                buf.pop_front();
            }
            buf.push_back(value);
            true
        } else {
            false
        }
    }

    /// Re-point the window at a new pid and clear every series.
    fn reset(&mut self, pid: u32) {
        self.pid = pid;
        self.cpu.clear();
        self.memory.clear();
        self.disk_read.clear();
        self.disk_write.clear();
        self.revision = self.revision.wrapping_add(1);
    }

    /// Re-point the window AND resize the capacity in one step (the persisted
    /// graph-data-points preference changed while tracking a process).
    pub fn resize(&mut self, capacity: usize, pid: u32) {
        self.capacity = capacity.clamp(2, MAX_HISTORY_CAPACITY);
        self.reset(pid);
    }

    /// Seed every series from the provider-pre-populated per-process history
    /// windows (G-14: Linux fills ~60 s of `ProcessItem::cpu_history` /
    /// `mem_history` / `disk_read_history` / `disk_write_history`). The ring
    /// is re-pointed at `pid` first, then the NEWEST tail of each window
    /// (bounded by the capacity) is pushed in arrival order — the same
    /// oldest-first shape the live sampling path produces, so the overlay's
    /// sparklines render the provider history immediately on open and the
    /// live samples continue as the extension. Non-finite samples are dropped
    /// by the same policy as [`Self::push`]; an all-empty seed leaves the
    /// ring empty (the caller keeps the live-only fallback).
    pub fn seed_from_provider(
        &mut self,
        pid: u32,
        capacity: usize,
        cpu: &[f32],
        memory: &[f32],
        disk_read: &[f32],
        disk_write: &[f32],
    ) {
        self.capacity = capacity.clamp(2, MAX_HISTORY_CAPACITY);
        self.reset(pid);
        let capacity = self.capacity;
        for value in newest_tail(cpu, capacity) {
            let _ = Self::push_one(&mut self.cpu, *value, capacity);
        }
        for value in newest_tail(memory, capacity) {
            let _ = Self::push_one(&mut self.memory, *value, capacity);
        }
        for value in newest_tail(disk_read, capacity) {
            let _ = Self::push_one(&mut self.disk_read, *value, capacity);
        }
        for value in newest_tail(disk_write, capacity) {
            let _ = Self::push_one(&mut self.disk_write, *value, capacity);
        }
    }

    /// The configured window capacity (after clamping).
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// The tracked pid (0 = never sampled).
    #[must_use]
    pub const fn pid(&self) -> u32 {
        self.pid
    }

    /// Monotonic data revision used by the Iced contiguous-series cache.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Copy the four bounded rings into shared contiguous slices. Callers that
    /// repaint repeatedly should use the Iced projection-cache component
    /// rather than invoking this directly on every frame.
    #[must_use]
    pub(crate) fn snapshot(&self) -> ProcessPerfHistorySnapshot {
        ProcessPerfHistorySnapshot {
            cpu: Rc::from(
                self.cpu
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            memory: Rc::from(
                self.memory
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            disk_read: Rc::from(
                self.disk_read
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
            disk_write: Rc::from(
                self.disk_write
                    .iter()
                    .copied()
                    .collect::<Vec<_>>()
                    .into_boxed_slice(),
            ),
        }
    }

    /// CPU% series, oldest first.
    #[must_use]
    pub fn cpu_samples(&self) -> Vec<f32> {
        self.cpu.iter().copied().collect()
    }

    /// Memory bytes series, oldest first.
    #[must_use]
    pub fn memory_samples(&self) -> Vec<f32> {
        self.memory.iter().copied().collect()
    }

    /// Disk read rate series, oldest first.
    #[must_use]
    pub fn disk_read_samples(&self) -> Vec<f32> {
        self.disk_read.iter().copied().collect()
    }

    /// Disk write rate series, oldest first.
    #[must_use]
    pub fn disk_write_samples(&self) -> Vec<f32> {
        self.disk_write.iter().copied().collect()
    }

    /// True when every series is empty (the honest "collecting" branch).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.cpu.is_empty()
            && self.memory.is_empty()
            && self.disk_read.is_empty()
            && self.disk_write.is_empty()
    }
}

#[cfg(test)]
#[path = "../tests/gui/perf_history_tests.rs"]
mod tests;
