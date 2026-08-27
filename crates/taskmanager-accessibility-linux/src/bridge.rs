//! Linux AT-SPI accessibility bridge backed by [`accesskit_unix::Adapter`].
//!
//! This is the real, non-stub publication path for TaskForest on Linux. It owns
//! one `accesskit_unix::Adapter` (which itself spawns a single process-global
//! background thread that speaks AT-SPI over the D-Bus session bus) and exposes
//! it through the toolkit-neutral [`AccessibilityBridge`] trait.
//!
//! Key properties:
//!
//! * **No window handle is required.** `accesskit_unix::Adapter::new` registers
//!   on the AT-SPI session bus at the process level; it never asks for a
//!   `wl_surface` or X11 window. This is what makes the bridge viable under
//!   gpui's Wayland backend, which keeps its surface internal.
//! * **Lazy publication.** [`Adapter::update_if_active`] short-circuits to a
//!   no-op while no assistive technology is registered on the bus, so calling
//!   [`AccessibilityBridge::try_publish`] on every UI tick is free when no
//!   screen reader is running. The `TreeUpdate` factory closure is only ever
//!   invoked once an AT has activated the tree.
//! * **Graceful degradation.** If there is no session D-Bus at all, the adapter
//!   thread simply never connects; construction and `try_publish` never panic.

#![cfg(target_os = "linux")]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use accesskit::{
    ActionData, ActionHandler, ActionRequest, ActivationHandler, DeactivationHandler, TreeUpdate,
};
use accesskit_unix::Adapter;
use taskmanager_ui_contract::{
    AccessibilityActionRequest, AccessibilityBridge, AccessibilityBridgeCapability,
    AccessibilityBridgeError, AccessibilityBridgeFeatures, AccessibilityPublication,
    SemanticSnapshot,
};

use crate::mapping::{semantic_id_for, snapshot_to_tree_update, unmap_action};

/// Latest snapshot shared between the render thread (which publishes) and the
/// adapter's background thread (which serves the initial tree to a newly-active
/// AT and resolves inbound action targets).
struct SharedState {
    snapshot: Option<SemanticSnapshot>,
}

/// Inbound action queue shared between the background `ActionHandler` thread and
/// the render thread's `try_recv_action` drain.
type ActionQueue = Arc<Mutex<VecDeque<AccessibilityActionRequest>>>;

/// Provides the full tree when an assistive technology first subscribes.
struct LinuxActivationHandler {
    shared: Arc<Mutex<SharedState>>,
}

impl ActivationHandler for LinuxActivationHandler {
    fn request_initial_tree(&mut self) -> Option<TreeUpdate> {
        let guard = self.shared.lock().ok()?;
        guard.snapshot.as_ref().map(snapshot_to_tree_update)
    }
}

/// Translates accesskit action requests into semantic action requests and
/// pushes them onto the shared queue for the render thread to drain.
struct LinuxActionHandler {
    queue: ActionQueue,
    shared: Arc<Mutex<SharedState>>,
}

impl ActionHandler for LinuxActionHandler {
    fn do_action(&mut self, request: ActionRequest) {
        let Some(action) = unmap_action(request.action) else {
            return;
        };

        let Ok(shared) = self.shared.lock() else {
            return;
        };
        let Some(snapshot) = shared.snapshot.as_ref() else {
            return;
        };
        let Some(node) = semantic_id_for(snapshot, request.target_node) else {
            return;
        };

        // Surface a value only for value-bearing actions; ignore unrelated
        // payload variants so the contract's validate step sees what it expects.
        let value = match request.data {
            Some(ActionData::Value(boxed)) => Some(boxed.into_string()),
            Some(ActionData::NumericValue(n)) => Some(n.to_string()),
            _ => None,
        };

        let request = AccessibilityActionRequest {
            snapshot_revision: snapshot.revision(),
            node,
            action,
            value,
        };
        drop(shared);
        if let Ok(mut queue) = self.queue.lock() {
            // Bound the queue so a misbehaving AT cannot grow it without limit;
            // the newest requests are what matter for interactive feedback.
            if queue.len() >= ACTION_QUEUE_CAPACITY {
                queue.pop_front();
            }
            queue.push_back(request);
        }
    }
}

/// Drops the stashed snapshot when the last assistive technology unsubscribes.
struct LinuxDeactivationHandler {
    shared: Arc<Mutex<SharedState>>,
}

impl DeactivationHandler for LinuxDeactivationHandler {
    fn deactivate_accessibility(&mut self) {
        if let Ok(mut shared) = self.shared.lock() {
            shared.snapshot = None;
        }
    }
}

/// Maximum inbound action requests buffered between render-thread drains. The
/// render loop drains every UI tick, so this only bounds a flood.
const ACTION_QUEUE_CAPACITY: usize = 64;

/// Real Linux AT-SPI accessibility bridge.
///
/// Construct one per application (typically alongside the main window's
/// [`RootView`](../../taskmanager-gpui/gpui_app/root/struct.RootView.html)) and call
/// [`AccessibilityBridge::try_publish`] on each semantic revision. The bridge is
/// `Send + Sync`: the adapter is guarded by a `Mutex` and the cross-thread state
/// by `Arc<Mutex<..>>`.
pub struct LinuxAccessKitBridge {
    adapter: Mutex<Adapter>,
    shared: Arc<Mutex<SharedState>>,
    queue: ActionQueue,
}

impl Default for LinuxAccessKitBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl LinuxAccessKitBridge {
    /// Construct the bridge and spawn the (lazily-connected) AT-SPI background
    /// thread. This never blocks on the bus: `accesskit_unix` only reaches the
    /// session bus from its worker thread, and only after an AT has enabled
    /// accessibility on the desktop.
    #[must_use]
    pub fn new() -> Self {
        let shared = Arc::new(Mutex::new(SharedState { snapshot: None }));
        let queue: ActionQueue = Arc::new(Mutex::new(VecDeque::new()));

        let adapter = Adapter::new(
            LinuxActivationHandler {
                shared: Arc::clone(&shared),
            },
            LinuxActionHandler {
                queue: Arc::clone(&queue),
                shared: Arc::clone(&shared),
            },
            LinuxDeactivationHandler {
                shared: Arc::clone(&shared),
            },
        );

        Self {
            adapter: Mutex::new(adapter),
            shared,
            queue,
        }
    }
}

impl AccessibilityBridge for LinuxAccessKitBridge {
    fn capability(&self) -> AccessibilityBridgeCapability {
        // The bridge is initialized and able to publish right away. Whether an
        // AT is actively listening is accesskit's internal lazy state, not a
        // readiness gate; we never claim readiness from a bus marker alone.
        AccessibilityBridgeCapability::ready(AccessibilityBridgeFeatures {
            actions: true,
            live_regions: true,
            tables: true,
            // The graph is published as a read-only Meter; the AT reads the
            // spoken value rather than navigating earlier/later samples.
            graph_navigation: false,
        })
    }

    fn try_publish(
        &self,
        snapshot: SemanticSnapshot,
    ) -> Result<AccessibilityPublication, AccessibilityBridgeError> {
        let revision = snapshot.revision();

        // Stash the snapshot for the background activation/action handlers.
        // One clone per publish: the tree is small (tens of nodes).
        if let Ok(mut shared) = self.shared.lock() {
            shared.snapshot = Some(snapshot.clone());
        }

        // Drive the adapter. The closure is only invoked when an AT is actively
        // reading; otherwise this is a no-op that touches neither the bus nor
        // the snapshot mapping. The guard is dropped before this call so the
        // background thread can never deadlock against us on `shared`.
        if let Ok(mut adapter) = self.adapter.lock() {
            adapter.update_if_active(|| snapshot_to_tree_update(&snapshot));
            // Single-window application: when we are rendering, our window has
            // keyboard focus. This drives AT-SPI focus events without claiming
            // any surface handle we do not own on Wayland.
            adapter.update_window_focus_state(true);
        }

        Ok(AccessibilityPublication {
            snapshot_revision: revision,
        })
    }

    fn try_recv_action(
        &self,
    ) -> Result<Option<AccessibilityActionRequest>, AccessibilityBridgeError> {
        let request = self
            .queue
            .lock()
            .map_err(|_| AccessibilityBridgeError::Backpressure)?
            .pop_front();
        Ok(request)
    }
}
