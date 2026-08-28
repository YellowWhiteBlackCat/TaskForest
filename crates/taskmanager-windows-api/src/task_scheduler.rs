//! Startup task enumeration through the Task Scheduler COM API.
//!
//! The Task Scheduler service is the Windows counterpart of a user-session
//! init system: logon/boot-triggered tasks are a real startup source and the
//! neutral contract already carries [`StartupSource::ScheduledTask`] (core).
//! Enumeration goes through `ITaskService` → root folder (+ one bounded level
//! of subfolders) → `IRegisteredTask` state and triggers, unprivileged for
//! readable tasks; access-denied folders/tasks degrade typed, and a stopped
//! or absent Task Scheduler service surfaces as an honest failure — never an
//! empty-but-healthy list.
//!
//! This module is read-only inventory: enabling/disabling a task mutates the
//! task store and stays out of scope until a control seam is chartered.

use crate::WindowsApiError;

/// One task-scheduler task relevant as a startup source.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsStartupTask {
    /// Backslash-joined folder path + task name (stable identity).
    pub task_path: String,
    /// Display name when the task carries one.
    pub name: Option<String>,
    /// Whether the task is currently enabled.
    pub enabled: bool,
    /// Whether any trigger is a logon or boot trigger (startup relevance
    /// filter; tasks with only time/event triggers are not startup items).
    pub has_logon_or_boot_trigger: bool,
}

/// Enumerate registered tasks, filtered to logon/boot-triggered ones,
/// from the root folder plus one bounded level of subfolders.
#[cfg(windows)]
pub fn enumerate_startup_tasks() -> Result<Vec<WindowsStartupTask>, WindowsApiError> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx,
        CoUninitialize,
    };
    use windows::Win32::System::TaskScheduler::{ITaskService, TaskScheduler};
    use windows::Win32::System::Variant::VARIANT;
    use windows::core::BSTR;

    // SAFETY: COM initialization on the calling thread, multi-threaded
    // apartment like the other COM lanes. A failure (for example
    // RPC_E_CHANGED_MODE when the host thread already chose an apartment)
    // still leaves COM usable on this thread, so enumeration proceeds and the
    // guard pairs CoUninitialize with a successful init only.
    let hr = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let needs_uninit = hr.is_ok();

    struct ComGuard(bool);
    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.0 {
                // SAFETY: CoUninitialize matches a successful CoInitializeEx
                // on this thread; Drop runs at most once.
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }
    let _com_guard = ComGuard(needs_uninit);

    let service: ITaskService =
        // SAFETY: the documented Task Scheduler 2.0 class with in-proc server
        // context is the sanctioned unprivileged route (the in-proc server
        // proxies to the service over RPC); the returned interface is owned.
        unsafe { CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER) }
            .map_err(com_failure)?;

    // Empty variants connect to the local machine as the calling user — no
    // credentials are captured or reconstructed here.
    let empty = VARIANT::default();
    // SAFETY: `service` is a valid ITaskService; the four empty VARIANTs are
    // synchronous by-value arguments the callee does not retain. A stopped
    // Task Scheduler service surfaces as this call's typed failure instead of
    // an empty-but-healthy list.
    unsafe { service.Connect(&empty, &empty, &empty, &empty) }.map_err(com_failure)?;

    // SAFETY: connected service; the root path is a fixed one-character BSTR.
    let root = unsafe { service.GetFolder(&BSTR::from("\\")) }.map_err(com_failure)?;

    let mut tasks = Vec::new();
    let mut visited = 0usize;
    collect_folder_tasks(&root, &mut tasks, &mut visited)?;

    // SAFETY: valid folder; flags 0 lists non-hidden subfolders.
    let subfolders = unsafe { root.GetFolders(0) }.map_err(com_failure)?;
    // SAFETY: valid collection.
    let folder_count = unsafe { subfolders.Count() }.map_err(com_failure)?;
    if folder_count < 0 || folder_count as usize > MAX_TASK_SCHEDULER_FOLDERS {
        return Err(WindowsApiError::ResourceLimit);
    }
    for index in 1..=folder_count {
        // SAFETY: valid collection; `index_variant` builds the documented
        // 1-based VT_I4 index argument.
        let folder = unsafe { subfolders.get_Item(&index_variant(index)) }.map_err(com_failure)?;
        collect_folder_tasks(&folder, &mut tasks, &mut visited)?;
    }
    Ok(tasks)
}

/// Subfolders visited below the root folder (one bounded level).
#[cfg(windows)]
const MAX_TASK_SCHEDULER_FOLDERS: usize = 16;

/// Total registered tasks accepted across all visited folders before the
/// enumeration degrades to [`WindowsApiError::ResourceLimit`].
#[cfg(windows)]
const MAX_TASK_SCHEDULER_TASKS: usize = 256;

/// Maximum trigger records inspected for one registered task.
#[cfg(windows)]
const MAX_TASK_SCHEDULER_TRIGGERS: i32 = 256;

/// Append this folder's startup-relevant tasks, honoring the global bound.
#[cfg(windows)]
fn collect_folder_tasks(
    folder: &windows::Win32::System::TaskScheduler::ITaskFolder,
    tasks: &mut Vec<WindowsStartupTask>,
    visited: &mut usize,
) -> Result<(), WindowsApiError> {
    // SAFETY: valid folder; flags 0 lists the non-hidden registered tasks the
    // calling user may see.
    let registered = unsafe { folder.GetTasks(0) }.map_err(com_failure)?;
    // SAFETY: valid collection.
    let count = unsafe { registered.Count() }.map_err(com_failure)?;
    if count < 0 {
        return Err(WindowsApiError::ResourceLimit);
    }
    let count_usize = usize::try_from(count).map_err(|_| WindowsApiError::ResourceLimit)?;
    let total = visited
        .checked_add(count_usize)
        .ok_or(WindowsApiError::ResourceLimit)?;
    if total > MAX_TASK_SCHEDULER_TASKS {
        return Err(WindowsApiError::ResourceLimit);
    }
    for index in 1..=count {
        // SAFETY: valid collection; 1-based VT_I4 index argument.
        let task = unsafe { registered.get_Item(&index_variant(index)) }.map_err(com_failure)?;
        if let Some(task) = startup_relevant_task(&task)? {
            tasks.push(task);
        }
        *visited = visited
            .checked_add(1)
            .ok_or(WindowsApiError::ResourceLimit)?;
    }
    Ok(())
}

/// Read one registered task's stable identity and startup relevance. Tasks
/// whose readable triggers contain no logon/boot trigger are not startup
/// items and stay absent rather than being presented with a guessed flag.
#[cfg(windows)]
fn startup_relevant_task(
    task: &windows::Win32::System::TaskScheduler::IRegisteredTask,
) -> Result<Option<WindowsStartupTask>, WindowsApiError> {
    use windows::Win32::Foundation::VARIANT_FALSE;
    use windows::Win32::System::TaskScheduler::{
        TASK_TRIGGER_BOOT, TASK_TRIGGER_LOGON, TASK_TRIGGER_TYPE2,
    };

    // SAFETY: valid registered-task interface; each call is a synchronous
    // property read whose result is an owned value.
    let task_path = unsafe { task.Path() }.map_err(com_failure)?.to_string();
    // SAFETY: same.
    let name = unsafe { task.Name() }.map_err(com_failure)?.to_string();
    // SAFETY: same.
    let enabled = unsafe { task.Enabled() }.map_err(com_failure)? != VARIANT_FALSE;
    // SAFETY: same; Definition returns the stored definition.
    let definition = unsafe { task.Definition() }.map_err(com_failure)?;
    // SAFETY: valid definition; Triggers returns the trigger collection.
    let triggers = unsafe { definition.Triggers() }.map_err(com_failure)?;
    let trigger_count = {
        let mut count = 0_i32;
        // SAFETY: valid collection; Count writes the element count.
        unsafe { triggers.Count(&mut count) }.map_err(com_failure)?;
        count
    };
    if trigger_count < 0 {
        return Err(WindowsApiError::QueryFailed);
    }
    if trigger_count > MAX_TASK_SCHEDULER_TRIGGERS {
        return Err(WindowsApiError::ResourceLimit);
    }
    let mut has_logon_or_boot_trigger = false;
    for index in 1..=trigger_count {
        // SAFETY: valid collection; 1-based index argument.
        let trigger = unsafe { triggers.get_Item(index) }.map_err(com_failure)?;
        let trigger_type = {
            let mut kind = TASK_TRIGGER_TYPE2::default();
            // SAFETY: valid trigger interface; Type writes the discriminant.
            unsafe { trigger.Type(&mut kind) }.map_err(com_failure)?;
            kind
        };
        if trigger_type == TASK_TRIGGER_LOGON || trigger_type == TASK_TRIGGER_BOOT {
            has_logon_or_boot_trigger = true;
        }
    }
    if !has_logon_or_boot_trigger {
        return Ok(None);
    }
    Ok(Some(WindowsStartupTask {
        task_path,
        name: (!name.is_empty()).then_some(name),
        enabled,
        has_logon_or_boot_trigger: true,
    }))
}

/// Build the 1-based `VT_I4` collection-index argument the Task Scheduler
/// collections expect.
#[cfg(windows)]
fn index_variant(index: i32) -> windows::Win32::System::Variant::VARIANT {
    windows::Win32::System::Variant::VARIANT::from(index)
}

/// Collapse a COM failure onto the typed boundary error: access denial is
/// distinguishable, and everything else — including a stopped Task Scheduler
/// service, which the adapter surfaces as temporarily unavailable — stays a
/// plain query failure. Never an empty success.
#[cfg(windows)]
fn com_failure(error: windows::core::Error) -> WindowsApiError {
    use windows::Win32::Foundation::E_ACCESSDENIED;
    if error.code() == E_ACCESSDENIED {
        WindowsApiError::PermissionDenied
    } else {
        WindowsApiError::QueryFailed
    }
}

/// Non-Windows hosts keep the lane dormant with the typed fallback.
#[cfg(not(windows))]
pub fn enumerate_startup_tasks() -> Result<Vec<WindowsStartupTask>, WindowsApiError> {
    Err(WindowsApiError::Unsupported)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_task_scheduler.rs"]
mod tests;
