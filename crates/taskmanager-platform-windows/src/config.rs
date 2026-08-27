//! Windows selection of the user configuration path.

use std::path::PathBuf;

/// Build a persistent Windows AppData directory from `USERPROFILE` only when
/// the profile is absolute. This is a compatibility fallback for restricted
/// shells, service-like launches, and portable test hosts where the Known
/// Folder API and APPDATA/LOCALAPPDATA may be unavailable. It deliberately
/// stays outside `temp_dir`: preferences and history must not disappear when
/// Windows changes or cleans the temporary directory.
fn profile_app_data_directory(profile: PathBuf, leaf: &str) -> Option<PathBuf> {
    (!profile.as_os_str().is_empty() && profile.is_absolute())
        .then(|| profile.join("AppData").join(leaf))
}

#[cfg(windows)]
fn absolute_directory(path: PathBuf) -> Option<PathBuf> {
    path.is_absolute()
        .then_some(path)
        .filter(|path| !path.as_os_str().is_empty())
}

/// Resolve the TaskForest configuration path using the native Windows Known
/// Folder API. Environment variables remain a narrow compatibility fallback;
/// production never silently writes preferences relative to the process
/// working directory. If Windows cannot resolve either native or environment
/// data, the absolute temp directory is an explicitly ephemeral last resort.
#[must_use]
pub fn user_config_path() -> PathBuf {
    #[cfg(windows)]
    {
        let base = taskmanager_windows_api::known_folder_path(
            taskmanager_windows_api::KnownFolder::RoamingAppData,
        )
        .ok()
        .or_else(|| {
            std::env::var_os("APPDATA")
                .map(PathBuf::from)
                .and_then(absolute_directory)
        })
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .and_then(absolute_directory)
        })
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .and_then(|profile| profile_app_data_directory(profile, "Roaming"))
                .and_then(absolute_directory)
        })
        .unwrap_or_else(std::env::temp_dir);
        base.join("TaskForest").join("config.json")
    }

    #[cfg(not(windows))]
    {
        let base = std::env::var_os("APPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("LOCALAPPDATA")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
            })
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .and_then(|profile| profile_app_data_directory(profile, "Roaming"))
            });
        base.map_or_else(
            || PathBuf::from("taskmanager-config.json"),
            |directory| directory.join("TaskForest").join("config.json"),
        )
    }
}

/// Resolve the persistent telemetry-history directory (roadmap #4, ADR-028)
/// through the native Local AppData Known Folder. The absolute temp directory
/// is the explicit non-persistent fallback when a restricted environment has
/// no usable user-data root.
#[must_use]
pub fn user_history_dir() -> PathBuf {
    #[cfg(windows)]
    {
        let base = taskmanager_windows_api::known_folder_path(
            taskmanager_windows_api::KnownFolder::LocalAppData,
        )
        .ok()
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(PathBuf::from)
                .and_then(absolute_directory)
        })
        .or_else(|| {
            std::env::var_os("USERPROFILE")
                .map(PathBuf::from)
                .and_then(|profile| profile_app_data_directory(profile, "Local"))
                .and_then(absolute_directory)
        })
        .unwrap_or_else(std::env::temp_dir);
        base.join("TaskForest").join("history")
    }

    #[cfg(not(windows))]
    {
        let base = std::env::var_os("LOCALAPPDATA")
            .filter(|value| !value.is_empty())
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("USERPROFILE")
                    .filter(|value| !value.is_empty())
                    .map(PathBuf::from)
                    .and_then(|profile| profile_app_data_directory(profile, "Local"))
            });
        base.map_or_else(
            || PathBuf::from("taskmanager-history"),
            |directory| directory.join("TaskForest").join("history"),
        )
    }
}

/// Resolve the user's Windows locale through the native safe API boundary.
///
/// This is a hint for frontend default-language selection only. A persisted
/// user preference always wins, and a missing/invalid native value remains a
/// typed absence rather than silently changing an existing preference.
#[must_use]
pub fn user_locale_name() -> Option<String> {
    taskmanager_windows_api::user_locale_name().ok()
}

#[cfg(test)]
#[path = "../tests/headless/platform_windows_config.rs"]
mod tests;
