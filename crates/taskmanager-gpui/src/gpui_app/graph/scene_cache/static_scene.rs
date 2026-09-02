//! Static graph grid and fill scene cache.

use super::{
    Background, Bounds, GraphOpts, GraphSceneCache, GraphStaticSceneEntry, GraphStaticSceneKey,
    MAX_GRAPH_STATIC_SCENE_ENTRIES, Pixels, Rgba, Window, build_graph_static_geometry,
    evict_static, rgba_bits,
};

/// Look up (or build and store) the static grid/fill scene for one canvas,
/// paint it, and return the primary series' fill background. The static key
/// carries no sample identity, so it is assembled directly from the
/// bounds/theme/options inputs.
pub(super) fn paint_graph_static_scene(
    cache: &mut GraphSceneCache,
    window: &mut Window,
    bounds: Bounds<Pixels>,
    base: Rgba,
    opts: GraphOpts,
) -> Background {
    let static_key = GraphStaticSceneKey {
        origin: (f32::from(bounds.origin.x), f32::from(bounds.origin.y)),
        size: (f32::from(bounds.size.width), f32::from(bounds.size.height)),
        theme_key: rgba_bits(base),
        fill_alpha_bits: opts.fill_alpha.to_bits(),
        grid_alpha_bits: opts.grid_alpha.to_bits(),
        hlines: opts.hlines,
        vlines: opts.vlines,
        stroke_width_bits: opts.stroke_width.to_bits(),
        gradient_fill: opts.gradient_fill,
        ref_lines: opts.ref_lines,
    };
    let store = &mut cache.static_scenes;
    let index = match store.iter().position(|entry| entry.key == static_key) {
        Some(index) => index,
        None => {
            evict_static(store, MAX_GRAPH_STATIC_SCENE_ENTRIES);
            store.push(GraphStaticSceneEntry {
                key: static_key,
                geometry: build_graph_static_geometry(bounds, base, opts),
            });
            store.len() - 1
        }
    };
    let entry = &store[index];
    entry.geometry.paint(window);
    entry.geometry.fill()
}
