# bsn! authoring law

The 100% `bsn!` law is a production authoring constraint, not a style
preference. `content`, page subtrees, rows/cells, widgets, overlays and all
loading/empty/error variants must be `*_scene` adapters returning `Scene` (or
composed scene values) and must enter the World through `spawn_scene`. Direct
UI `Node`/`Children` construction, `with_children`, or imperative child
spawning is forbidden in production code. ECS systems may update typed
components on scene-owned entities, wire events/focus, or despawn and replace
a bounded subtree with another `bsn!` scene. Headless fixtures and Bevy's own
plugin internals are not production UI authoring routes.

For scene polymorphism, follow Bevy 0.20's boundary: fixed composition uses
`impl Scene`, homogeneous lists use `Vec<S>`, and `Vec<Box<dyn Scene>>` is
reserved for runtime-filtered or heterogeneous children. Every boxed value
still comes from a `bsn!` Scene adapter; boxing is a type-erasure tool, not a
second UI construction route. Stable telemetry scenes update marked
components in place instead of allocating and rebuilding on every frame.

`src/window.rs` is the bsn! idiom reference for this crate: the route-aware
app shell (route-aware shell + nav rail + content slot), including the shared
MiSans VF UI and Roboto Mono telemetry font roles and its observers.

The dependency whitelist, the exactly-locked Bevy version and the two-Worlds
rule are charter law — see [`BEVY_UI_FRONTEND`](BEVY_UI_FRONTEND.md) 基座与边界铁律.
