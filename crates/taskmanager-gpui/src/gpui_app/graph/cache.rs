//! Per-window graph presentation cache.

use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

use gpui::{Bounds, Path, Pixels, Rgba};

use super::{GraphSettings, scene_cache, slide};

/// One memoized tail slice: the source projection plus the exact `limit` it
/// was cut to. The source `Rc` is pinned so a recycled address cannot serve a
/// stale slice.
struct TailSliceEntry {
    source: Rc<[f32]>,
    limit: usize,
    slice: Rc<[f32]>,
}

const MAX_TAIL_SLICE_ENTRIES: usize = 512;

/// All mutable graph presentation caches owned by one GPUI window.
///
/// The handle lives on `RootView` and is cloned into canvas closures. Keeping
/// the caches here makes their lifetime and isolation explicit: a second
/// window cannot observe a first window's graph scenes, slide clocks, sample
/// projections, or hover-refresh budget, while the closures still outlive the
/// render call safely.
#[derive(Default)]
pub(crate) struct GraphPresentationCache {
    tail_slices: Vec<TailSliceEntry>,
    scenes: scene_cache::GraphSceneCache,
    slides: slide::SlideCache,
    samples: crate::gpui_app::history_samples::DeviceSampleCache,
    last_hover_refresh: Option<Instant>,
}

pub(crate) type GraphCacheHandle = Rc<RefCell<GraphPresentationCache>>;

#[must_use]
pub(crate) fn new_graph_cache() -> GraphCacheHandle {
    Rc::new(RefCell::new(GraphPresentationCache::default()))
}

impl GraphPresentationCache {
    pub(crate) fn latest_samples(
        &mut self,
        samples: Rc<[f32]>,
        data_points: usize,
        sliding: bool,
    ) -> Rc<[f32]> {
        let limit = GraphSettings::clamp_data_points(data_points).saturating_add(if sliding {
            1
        } else {
            0
        });
        if samples.len() <= limit {
            return samples;
        }
        if let Some(entry) = self
            .tail_slices
            .iter()
            .find(|entry| entry.limit == limit && Rc::ptr_eq(&entry.source, &samples))
        {
            return Rc::clone(&entry.slice);
        }
        if self.tail_slices.len() >= MAX_TAIL_SLICE_ENTRIES {
            self.tail_slices
                .retain(|entry| Rc::strong_count(&entry.source) > 1);
            if self.tail_slices.len() >= MAX_TAIL_SLICE_ENTRIES {
                self.tail_slices.clear();
            }
        }
        let slice = Rc::from(&samples[samples.len() - limit..]);
        self.tail_slices.push(TailSliceEntry {
            source: samples,
            limit,
            slice: Rc::clone(&slice),
        });
        slice
    }

    pub(crate) fn scenes_mut(&mut self) -> &mut scene_cache::GraphSceneCache {
        &mut self.scenes
    }

    pub(crate) fn slides_mut(&mut self) -> &mut slide::SlideCache {
        &mut self.slides
    }

    pub(crate) fn sparkline_paths(
        &mut self,
        samples: &Rc<[f32]>,
        bounds: Bounds<Pixels>,
        color: Rgba,
    ) -> Vec<Path<Pixels>> {
        scene_cache::sparkline_paths(self.scenes_mut(), samples, bounds, color)
    }

    pub(crate) fn with_device_samples<R>(
        &mut self,
        access: impl FnOnce(&mut crate::gpui_app::history_samples::DeviceSampleCache) -> R,
    ) -> R {
        access(&mut self.samples)
    }

    pub(super) fn hover_refresh_due(&mut self, now: Instant) -> bool {
        let due = scene_cache::hover_refresh_is_due(
            self.last_hover_refresh,
            now,
            scene_cache::MIN_HOVER_REFRESH_INTERVAL,
        );
        if due {
            self.last_hover_refresh = Some(now);
        }
        due
    }

    pub(super) fn reset_hover_refresh(&mut self) {
        self.last_hover_refresh = None;
    }
}
