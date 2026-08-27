//! GPUI's adapter for the toolkit-neutral embedded asset store.

use std::borrow::Cow;
use std::collections::BTreeSet;

use gpui::{AssetSource, Result, SharedString};

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TaskManagerAssets;

impl AssetSource for TaskManagerAssets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(taskmanager_assets::asset_bytes(path).map(Cow::Borrowed))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        let directory = path.trim_matches('/');
        let prefix = if directory.is_empty() {
            String::new()
        } else {
            format!("{directory}/")
        };
        let mut children = BTreeSet::new();
        for asset_path in taskmanager_assets::all_asset_paths() {
            let Some(remainder) = asset_path.strip_prefix(&prefix) else {
                continue;
            };
            let child = remainder.split('/').next().unwrap_or_default();
            if !child.is_empty() {
                children.insert(SharedString::from(child));
            }
        }
        Ok(children.into_iter().collect())
    }
}

#[cfg(test)]
#[path = "../tests/gui/gpui_assets_tests.rs"]
mod tests;
