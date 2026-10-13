# taskmanager-gpui

## Role

GPUI desktop frontend product (`taskforest-g`, ADR-051). It owns `RootView`,
window-local presentation, page rendering, focus, overlays and GPUI capture scenes.
Its binary hands product capabilities, including Windows `--capture-window`, to
`taskmanager-cli`.

## Boundary

The frontend consumes immutable application/shell projections and submits typed
intents. Providers and native I/O remain in platform runtime and app-host; no
renderer or keyboard handler reads process files, native handles or host clocks.
Local dates use the app-host time-zone observation and shared pure formatters.

The desktop host preserves the complete standalone shell. It uses GPUI Kit's
version-matched GPUI snapshot while retaining TaskForest's own component and
theme layers. On Linux/Wayland,
`TASKFOREST_WINDOW_HOST=layer-shell` requests the app-host presentation
contract's layer-shell Dashboard widget (profile owned by
`taskmanager-app-host`, see its
[`docs/WINDOW_PRESENTATION.md`](../taskmanager-app-host/docs/WINDOW_PRESENTATION.md)).
The pinned GPUI snapshot owns the protocol role and configure/ack
lifecycle; unavailable protocols follow the explicit standalone fallback. Iced
and Bevy retain their independent host adapters.

`taskforest-g --demo` uses shared deterministic fixtures and bounded in-memory
history. It does not load user configuration, persist history, spawn a tray or
execute host actions. First Run opens through Settings or a user setup action;
capability discovery alone never creates a recurring modal.

## Contract and verification

Keep render-entry folds shared by keyboard and pointer. Verify behavior and side
effects through nextest; visible changes additionally require measured headless
bounds with product fonts and current native capture/validator/manual review.
Process insight captures use shared positive facts and require a rendered-frame
acknowledgement; opening a surface or having a file does not certify its contents.
Component implementations belong to `taskmanager-ui`.

## Module map

```text
src/assets.rs  capture.rs  run.rs      assets, evidence capture, binary entry
src/window_presentation.rs             layer-shell / standalone host adapter
src/gpui_app.rs                        RootView composition root
├── root/                              root view layout and lifecycle
│   ├── render.rs  render/             page rendering and shell composition
│   └── render/pages/                  page bodies (inventory, performance, apps, system)
├── chrome.rs  containers_view.rs      window skeleton and containers
├── sidebar/                           navigation sidebar
├── dashboard/                         dashboard page and cards
├── perf_views/                        performance page composition (ADR-039)
├── cpu_view/                          per-core CPU detail views
├── processes_view/                    process table page
├── process_insights/                  per-process detail overlay
├── services_view/                     services page
├── startup_view/                      startup items page
├── users_view/                        user sessions page
├── system_view/                       system information page
├── system_health_view/                system health page
├── settings_view/                     settings page
├── graph/                             shared graph/chart components
├── elements/                          shared UI elements
├── list_view.rs                       virtual list component
├── history_samples.rs  timeline.rs    history reads and application-curve allocation adapter
├── help_overlay.rs                    keyboard help overlay
├── system_about.rs  about.rs          system/app about pages
├── app_history_view.rs  first_run.rs  application history and first-run
├── functional.rs  capabilities.rs     CORE-04 declarations and capabilities
├── icons.rs  theme.rs  formatting.rs  icon bindings, theme, formatting
```

Consumes shell projections only; every frame drains a bounded projection cache.

## Docs

- [`docs/ROOT_STATE.md`](docs/ROOT_STATE.md) — projection materialization, caches, window
  surface, insights facets, history runtime and typed app-host clients.
- [`docs/PRESENTATION_CONTRACTS.md`](docs/PRESENTATION_CONTRACTS.md) — navigation, page
  composition, density, sidebar pinning, dashboard, startup, properties and tray behavior.
- [`docs/UI_GUIDE_GPUI.md`](docs/UI_GUIDE_GPUI.md) — GPUI widget and rendering guide.
- [`UI_COMPONENT_ARCHITECTURE`](../../docs/UI_COMPONENT_ARCHITECTURE.md) — cross-frontend
  component contracts; [ADR-038](../../adr/038-gpui-frame-content-budget.md) and
  [ADR-039](../../adr/039-performance-page-composition-root.md) — frame budget and perf page.
