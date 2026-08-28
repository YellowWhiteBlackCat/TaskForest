//! Per-process open-file listing over the system handle table (route B,
//! ADR-018).
//!
//! Windows has no `/proc/<pid>/fd`; the honest equivalent walks
//! `NtQuerySystemInformation(SystemHandleInformation)` (a kernel snapshot
//! that a standard user may query without opening the owning process),
//! duplicates each of the target's handles into this process with
//! `NtDuplicateObject` (needs `PROCESS_DUP_HANDLE` on the owner — same-user
//! processes only; anything else is a typed `PermissionDenied`), and resolves
//! type/name via `NtQueryObject`.
//!
//! Two hard constraints shape this module:
//!
//! 1. `NtQueryObject(ObjectNameInformation)` can block forever on handles
//!    opened for synchronous I/O (named pipes are the canonical case); the
//!    query has no timeout parameter. The only reliable mitigation — used by
//!    Process Hacker — is a sacrificial worker thread per call that is
//!    terminated after a small timeout (processhacker/processhacker#746).
//!    Every name resolution MUST run through that pattern; a plain call is a
//!    GUI freeze.
//! 2. Only `File`-type objects are open files. Kernel objects such as keys,
//!    events, sections, or tokens are NOT files and must never be relabelled
//!    as one (ADR-018); they are skipped, not classified as `Other`.
//!
//! The whole-system table can hold hundreds of thousands of entries, so the
//! walk is bounded and the per-process projection capped.
//!
//! `windows` 0.62 ships `NtQuerySystemInformation`/`NtQueryObject`, but not
//! the handle-table structs, `NtDuplicateObject` (behind a feature this crate
//! does not enable), or the `SystemHandleInformation`/name/all-types
//! information-class values. Those are declared here following the audited
//! `link!`/`repr(C)`-bytes precedent of `power.rs` and `memory_info.rs`.

use crate::WindowsApiError;

/// Coarse classification of one open `File`-object handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WindowsOpenHandleKind {
    /// A path-like target (file, directory, device node, ...).
    File,
    /// A named pipe (`\\.\pipe\...`).
    Pipe,
    /// A file object whose target could not be classified.
    Other,
}

/// One open `File`-object handle of the target process.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowsOpenHandleEntry {
    /// Native handle value.
    pub handle: u64,
    /// Coarse target classification.
    pub kind: WindowsOpenHandleKind,
    /// Resolved object name (usually an NT path like
    /// `\Device\HarddiskVolume3\...`), or `None` when the name query failed
    /// or timed out inside the sacrificial thread.
    pub target: Option<String>,
}

// ---------------------------------------------------------------------------
// Bounds (everything in this walk is bounded; exceeding any bound is a typed
// error or an honestly unresolved entry, never silent truncation).
// ---------------------------------------------------------------------------

/// Initial `SystemHandleInformation` buffer. Busy systems hold well over a
/// hundred thousand handles (24 bytes each on x64), so the table usually
/// needs a few doublings from here.
const HANDLE_TABLE_INITIAL_BYTES: usize = 1024 * 1024;
/// Hard ceiling for the handle-table buffer; doubling beyond it is refused
/// with a typed `ResourceLimit`.
const HANDLE_TABLE_MAX_BYTES: usize = 64 * 1024 * 1024;
/// Maximum buffer-growth attempts for the handle-table query.
const HANDLE_TABLE_MAX_ATTEMPTS: usize = 8;
/// Maximum number of `File` handles collected for one process. A process
/// exceeding it answers with a typed `ResourceLimit` instead of a silently
/// truncated listing.
const MAX_OPEN_FILE_ENTRIES_PER_PROCESS: usize = 1024;
/// Sacrificial-thread timeout for one name query.
const NAME_QUERY_TIMEOUT_MS: u32 = 100;
/// Grace period for a terminated sacrificial thread to signal before the
/// caller gives up on reclaiming its resources.
const NAME_QUERY_TERMINATION_GRACE_MS: u32 = 200;
/// Total wall-clock budget for all name resolutions of one call. Handles
/// still unresolved after it keep their entry with `target: None` and are
/// counted unreadable by the adapter.
const NAME_RESOLUTION_BUDGET_MS: u64 = 1000;
/// Per-name buffer in UTF-16 units. The kernel writes the
/// `OBJECT_NAME_INFORMATION` header plus the inline name string here; longer
/// names stay unresolved rather than being truncated.
const MAX_NAME_UTF16_UNITS: usize = 4096;
/// `SystemHandleInformation` information class (value not exported by the
/// `windows` crate).
const SYSTEM_HANDLE_INFORMATION_CLASS: i32 = 16;
/// `ObjectNameInformation` object-information class (value not exported).
const OBJECT_NAME_INFORMATION_CLASS: i32 = 1;
/// `ObjectAllTypes` object-information class (value not exported). Safe to
/// query with a NULL handle; only the name class can block.
const OBJECT_ALL_TYPES_CLASS: i32 = 3;
/// `STATUS_INFO_LENGTH_MISMATCH` (0xC0000004): the documented retry signal
/// for undersized `NtQuery*` buffers.
const STATUS_INFO_LENGTH_MISMATCH: i32 = -1_073_741_820;
/// `STATUS_UNSUCCESSFUL` (0xC0000001) doubles as the "worker has not written
/// its result yet" sentinel inside a name-query slot.
const NAME_STATUS_PENDING: i32 = -1_073_741_823;
/// Sanity ceiling for the object-type count parsed from `ObjectAllTypes`.
const MAX_OBJECT_TYPES: usize = 512;

// ---------------------------------------------------------------------------
// Host-independent layout arithmetic and pure walkers. Both hand-declared NT
// layouts derive from the target pointer width so 32- and 64-bit builds stay
// honest; every buffer read is bounds-checked and returns typed errors.
// ---------------------------------------------------------------------------

/// Pointer width of the compiled target in bytes.
const POINTER_BYTES: usize = core::mem::size_of::<usize>();

/// Round a byte offset up to the pointer alignment of the target.
const fn align_to_pointer(value: usize) -> usize {
    (value + POINTER_BYTES - 1) & !(POINTER_BYTES - 1)
}

/// Read the leading little-endian `ULONG` count of an NT-system-information
/// buffer. The slice conversion cannot fail on the four-byte window
/// `buffer.get(..4)` already validated, but the fold stays `try_into`-based
/// so no panic path exists even under refactor.
fn read_le_u32_prefix(buffer: &[u8]) -> Result<u32, WindowsApiError> {
    let Ok(bytes) = <[u8; 4]>::try_from(buffer.get(..4).unwrap_or_default()) else {
        return Err(WindowsApiError::QueryFailed);
    };
    Ok(u32::from_le_bytes(bytes))
}

/// Serialized `OBJECT_TYPE_INFORMATION` size: a `UNICODE_STRING` header (two
/// u16 lengths plus a pointer — two pointer widths with padding) followed by
/// the 88-byte documented statistics tail. Used as the per-entry stride base
/// of the `ObjectAllTypes` walk.
const OBJECT_TYPE_INFORMATION_TAIL_BYTES: usize = 88;
const fn object_type_information_bytes() -> usize {
    2 * POINTER_BYTES + OBJECT_TYPE_INFORMATION_TAIL_BYTES
}

/// Serialized `SYSTEM_HANDLE_TABLE_ENTRY_INFO` size:
/// `u16 UniqueProcessId, u16 CreatorBackTraceIndex, u8 ObjectType,
/// u8 HandleAttributes, u16 HandleValue, PVOID Object, ULONG GrantedAccess`
/// rounded up to the pointer alignment.
const HANDLE_ENTRY_PREFIX_BYTES: usize = 8;
const fn handle_entry_bytes() -> usize {
    align_to_pointer(HANDLE_ENTRY_PREFIX_BYTES + POINTER_BYTES + 4)
}

/// Byte offsets inside one serialized handle-table entry.
const HANDLE_ENTRY_PID_OFFSET: usize = 0;
const HANDLE_ENTRY_TYPE_OFFSET: usize = 4;
const HANDLE_ENTRY_VALUE_OFFSET: usize = 6;

/// Case-insensitive comparison of a UTF-16LE byte string against ASCII text.
/// The object namespace is case-preserving but case-insensitive, and the
/// compared literals (`File`, pipe prefixes) are pure ASCII, so an ASCII
/// case-fold over the low bytes with zero high bytes is exact here.
fn utf16_bytes_eq_ascii_ignore_case(bytes: &[u8], ascii: &[u8]) -> bool {
    if bytes.len() != ascii.len() * 2 {
        return false;
    }
    bytes
        .as_chunks::<2>()
        .0
        .iter()
        .zip(ascii)
        .all(|(unit, expected)| unit[1] == 0 && unit[0].eq_ignore_ascii_case(expected))
}

/// Locate the `File` object-type index inside an `ObjectAllTypes` buffer.
///
/// The buffer starts with `ULONG NumberOfTypes` followed by serialized
/// `OBJECT_TYPE_INFORMATION` entries, each holding the type name inline
/// after the fixed struct; entries advance by
/// `align_up(sizeof(OBJECT_TYPE_INFORMATION) + TypeName.MaximumLength)` —
/// the Process Hacker/systeminformer walk. A malformed layout is a typed
/// `QueryFailed`, never a panic or an out-of-bounds read.
fn find_file_type_index(buffer: &[u8]) -> Result<u16, WindowsApiError> {
    let count = read_le_u32_prefix(buffer)?;
    if count as usize > MAX_OBJECT_TYPES {
        return Err(WindowsApiError::QueryFailed);
    }
    let struct_bytes = object_type_information_bytes();
    let mut cursor = align_to_pointer(4);
    for index in 0..count {
        let Some(header) = buffer.get(cursor..cursor + 4) else {
            return Err(WindowsApiError::QueryFailed);
        };
        let name_length = u16::from_le_bytes([header[0], header[1]]) as usize;
        let name_capacity = u16::from_le_bytes([header[2], header[3]]) as usize;
        let name_start = cursor
            .checked_add(struct_bytes)
            .ok_or(WindowsApiError::QueryFailed)?;
        let name_end = name_start
            .checked_add(name_length)
            .ok_or(WindowsApiError::QueryFailed)?;
        let Some(name) = buffer.get(name_start..name_end) else {
            return Err(WindowsApiError::QueryFailed);
        };
        if utf16_bytes_eq_ascii_ignore_case(name, b"File") {
            return u16::try_from(index).map_err(|_| WindowsApiError::QueryFailed);
        }
        let next = cursor
            .checked_add(struct_bytes)
            .and_then(|value| value.checked_add(name_capacity))
            .ok_or(WindowsApiError::QueryFailed)?;
        cursor = align_to_pointer(next);
    }
    // Every real Windows system has a File object type; its absence means
    // the walk went wrong, which is a typed failure rather than "no files".
    Err(WindowsApiError::QueryFailed)
}

/// Extract the `File`-type handle values owned by `pid` from a
/// `SystemHandleInformation` buffer.
///
/// Returns the collected handle values and whether more matching entries
/// existed than `max_entries` (the caller turns that into a typed
/// `ResourceLimit`). The kernel reports the count imprecisely, so entries
/// are consumed only while a complete entry fits the buffer; a truncated
/// tail entry is left unread rather than half-parsed.
fn parse_target_file_handles(
    buffer: &[u8],
    pid: u32,
    file_type_index: u8,
    max_entries: usize,
) -> Result<(Vec<u16>, bool), WindowsApiError> {
    let count = read_le_u32_prefix(buffer)?;
    let entry_bytes = handle_entry_bytes();
    let mut cursor = align_to_pointer(4);
    let mut handles = Vec::new();
    let mut truncated = false;
    for _ in 0..count {
        let Some(entry) = buffer.get(cursor..cursor + entry_bytes) else {
            break;
        };
        cursor += entry_bytes;
        let entry_pid = u16::from_le_bytes([
            entry[HANDLE_ENTRY_PID_OFFSET],
            entry[HANDLE_ENTRY_PID_OFFSET + 1],
        ]);
        if u32::from(entry_pid) != pid {
            continue;
        }
        if entry[HANDLE_ENTRY_TYPE_OFFSET] != file_type_index {
            continue;
        }
        if handles.len() == max_entries {
            truncated = true;
            break;
        }
        handles.push(u16::from_le_bytes([
            entry[HANDLE_ENTRY_VALUE_OFFSET],
            entry[HANDLE_ENTRY_VALUE_OFFSET + 1],
        ]));
    }
    Ok((handles, truncated))
}

/// Classify a resolved File-object name into the coarse boundary kind.
/// Named-pipe NT namespaces (`\Device\NamedPipe\...`) and their Win32
/// spelling (`\\.\pipe\...`) are `Pipe`; any other named target is `File`;
/// an unresolvable or unnamed target is `Other`.
fn classify_named_target(name: Option<&str>) -> WindowsOpenHandleKind {
    let Some(name) = name else {
        return WindowsOpenHandleKind::Other;
    };
    const PIPE_PREFIXES: [&str; 2] = ["\\Device\\NamedPipe\\", "\\\\.\\pipe\\"];
    if PIPE_PREFIXES.iter().any(|prefix| {
        name.get(..prefix.len())
            .is_some_and(|head| head.eq_ignore_ascii_case(prefix))
    }) {
        return WindowsOpenHandleKind::Pipe;
    }
    WindowsOpenHandleKind::File
}

/// Reconstruct a pointer-sized value from its little-endian UTF-16 units.
fn pointer_from_units(units: &[u16]) -> Option<usize> {
    let max_units = usize::div_ceil(usize::BITS as usize, 16);
    if units.len() > max_units {
        return None;
    }
    let mut value = 0_usize;
    for (shift, unit) in units.iter().enumerate() {
        value |= usize::from(*unit) << (16 * shift);
    }
    Some(value)
}

/// Validate that a successful `NtQueryObject(ObjectNameInformation)` placed
/// the name inline after the in-buffer `UNICODE_STRING` header, and return
/// the name's length in UTF-16 units. A kernel that points elsewhere, or a
/// length that overruns the shared buffer, yields `None` (name unresolved)
/// instead of either field being trusted.
fn inline_name_units(buffer_addr: usize, buffer: &[u16]) -> Option<usize> {
    let header_units = POINTER_BYTES;
    if buffer.len() < header_units {
        return None;
    }
    let length_bytes = usize::from(buffer[0]);
    if length_bytes % 2 != 0 {
        return None;
    }
    let units = length_bytes / 2;
    if units == 0 {
        return Some(0);
    }
    let pointer_field_start = header_units / 2;
    let pointer_units = buffer.get(pointer_field_start..pointer_field_start + header_units / 2)?;
    if pointer_from_units(pointer_units) != buffer_addr.checked_add(2 * POINTER_BYTES) {
        return None;
    }
    if header_units + units > buffer.len() {
        return None;
    }
    Some(units)
}

/// Shared slot between the caller and one sacrificial name-query thread.
/// Worker write order is `status`, then the name copy, then `name_units`;
/// the caller reads only after the worker is confirmed finished or
/// terminated, so observing `status == 0 && name_units > 0` always implies
/// the copy completed.
struct NameQuerySlot {
    /// NTSTATUS of the worker's query; [`NAME_STATUS_PENDING`] until written.
    status: i32,
    /// Published name length in UTF-16 units (compacted to the slot start).
    name_units: usize,
    /// Query target buffer: the kernel writes header plus inline name here.
    name: [u16; MAX_NAME_UTF16_UNITS],
}

/// Decode a finished slot. `None` for a pending/failed/empty/oversized
/// result — the entry then carries `target: None` and the adapter counts it
/// unreadable.
fn decode_slot_name(slot: &NameQuerySlot) -> Option<String> {
    if slot.status != 0 {
        return None;
    }
    let units = slot.name_units.min(slot.name.len());
    if units == 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&slot.name[..units]))
}

// ---------------------------------------------------------------------------
// Windows FFI section.
// ---------------------------------------------------------------------------

/// List the target process's open `File`-object handles.
///
/// Returns a typed `PermissionDenied` when the owner cannot be opened for
/// handle duplication (other users' processes), `IdentityChanged` when the
/// process vanished, and `ResourceLimit` when the bounded walk over the
/// system handle table exceeded its budget.
#[cfg(windows)]
pub fn query_process_open_files(pid: u32) -> Result<Vec<WindowsOpenHandleEntry>, WindowsApiError> {
    use std::time::{Duration, Instant};

    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcess, PROCESS_DUP_HANDLE};

    /// RAII guard for the owner process handle opened for duplication.
    struct OwnerProcess(HANDLE);
    impl Drop for OwnerProcess {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by `OpenProcess` and is owned
            // exclusively by this guard; Drop runs at most once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    if pid == 0 {
        return Err(WindowsApiError::InvalidInput);
    }

    // Step 1: open the owner once. Other users' processes refuse
    // `PROCESS_DUP_HANDLE` — an honest typed scope, the same line Process
    // Explorer draws.
    let owner = {
        use windows::Win32::Foundation::{ERROR_ACCESS_DENIED, ERROR_INVALID_PARAMETER};
        // SAFETY: `pid` is a plain nonzero process identifier, inheritance
        // is refused, and the returned handle is immediately owned by the
        // RAII guard.
        let handle = unsafe { OpenProcess(PROCESS_DUP_HANDLE, false, pid) }.map_err(|error| {
            let code = error.code();
            if code == ERROR_ACCESS_DENIED.to_hresult() {
                WindowsApiError::PermissionDenied
            } else if code == ERROR_INVALID_PARAMETER.to_hresult() {
                WindowsApiError::IdentityChanged
            } else {
                WindowsApiError::QueryFailed
            }
        })?;
        OwnerProcess(handle)
    };

    // Step 2: resolve the File object-type index once for this call.
    let types = query_object_types_buffer()?;
    let file_type_index =
        u8::try_from(find_file_type_index(&types)?).map_err(|_| WindowsApiError::QueryFailed)?;

    // Step 3: walk the system handle table for the owner's File handles.
    let table = query_system_handle_table()?;
    let (handles, truncated) = parse_target_file_handles(
        &table,
        pid,
        file_type_index,
        MAX_OPEN_FILE_ENTRIES_PER_PROCESS,
    )?;
    if truncated {
        return Err(WindowsApiError::ResourceLimit);
    }

    // Step 4: duplicate and name each handle within the total budget.
    let deadline = Instant::now() + Duration::from_millis(NAME_RESOLUTION_BUDGET_MS);
    let current_process = {
        // SAFETY: returns the constant pseudo-handle for this process; it
        // is never closed.
        unsafe { GetCurrentProcess() }
    };
    let mut entries = Vec::with_capacity(handles.len());
    for handle_value in handles {
        let name = resolve_handle_name(owner.0, current_process, handle_value, &deadline);
        entries.push(WindowsOpenHandleEntry {
            handle: u64::from(handle_value),
            kind: classify_named_target(name.as_deref()),
            target: name,
        });
    }
    Ok(entries)
}

/// Non-Windows hosts keep the lane dormant with the typed fallback.
#[cfg(not(windows))]
pub fn query_process_open_files(_pid: u32) -> Result<Vec<WindowsOpenHandleEntry>, WindowsApiError> {
    Err(WindowsApiError::Unsupported)
}

/// Query the `SystemHandleInformation` snapshot with a bounded growth loop.
#[cfg(windows)]
fn query_system_handle_table() -> Result<Vec<u8>, WindowsApiError> {
    use windows::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SYSTEM_INFORMATION_CLASS,
    };

    let mut capacity_bytes = HANDLE_TABLE_INITIAL_BYTES;
    for _ in 0..HANDLE_TABLE_MAX_ATTEMPTS {
        let units = capacity_bytes / POINTER_BYTES;
        let mut buffer = vec![0_usize; units];
        let mut returned_bytes = 0_u32;
        // SAFETY: `buffer` is a writable, pointer-aligned allocation of
        // exactly `capacity_bytes` bytes whose length is passed in the
        // matching parameter; `returned_bytes` receives the kernel's byte
        // count. The documented `SystemHandleInformation` class opens no
        // handles and retains no caller pointers.
        let status = unsafe {
            NtQuerySystemInformation(
                SYSTEM_INFORMATION_CLASS(SYSTEM_HANDLE_INFORMATION_CLASS),
                buffer.as_mut_ptr().cast::<core::ffi::c_void>(),
                u32::try_from(capacity_bytes).map_err(|_| WindowsApiError::ResourceLimit)?,
                &mut returned_bytes,
            )
        };
        if status.is_ok() {
            let filled =
                usize::try_from(returned_bytes).map_err(|_| WindowsApiError::ResourceLimit)?;
            if filled == 0 || filled > capacity_bytes {
                return Err(WindowsApiError::QueryFailed);
            }
            // SAFETY: `filled` bytes of `buffer` were initialized by the
            // kernel and the u8 view reads them at a narrower alignment.
            return Ok(unsafe {
                core::slice::from_raw_parts(buffer.as_ptr().cast::<u8>(), filled).to_vec()
            });
        }
        if status.0 != STATUS_INFO_LENGTH_MISMATCH {
            return Err(WindowsApiError::QueryFailed);
        }
        if capacity_bytes >= HANDLE_TABLE_MAX_BYTES {
            return Err(WindowsApiError::ResourceLimit);
        }
        capacity_bytes = (capacity_bytes * 2).min(HANDLE_TABLE_MAX_BYTES);
    }
    Err(WindowsApiError::ResourceLimit)
}

/// Query `NtQueryObject(NULL, ObjectAllTypes)` with a small bounded growth
/// loop. The type list is a few dozen entries; only the name class can
/// block, so this runs directly on the caller thread.
#[cfg(windows)]
fn query_object_types_buffer() -> Result<Vec<u8>, WindowsApiError> {
    use windows::Wdk::Foundation::{NtQueryObject, OBJECT_INFORMATION_CLASS};

    const INITIAL_BYTES: usize = 8 * 1024;
    const MAX_BYTES: usize = 256 * 1024;
    const MAX_ATTEMPTS: usize = 6;

    let mut capacity_bytes = INITIAL_BYTES;
    for _ in 0..MAX_ATTEMPTS {
        let mut buffer = vec![0_u8; capacity_bytes];
        let mut returned_bytes = 0_u32;
        // SAFETY: a NULL handle requests global type information; `buffer`
        // is a writable allocation whose length is passed in the matching
        // parameter, and no caller pointer is retained.
        let status = unsafe {
            NtQueryObject(
                None,
                OBJECT_INFORMATION_CLASS(OBJECT_ALL_TYPES_CLASS),
                Some(buffer.as_mut_ptr().cast::<core::ffi::c_void>()),
                u32::try_from(capacity_bytes).map_err(|_| WindowsApiError::ResourceLimit)?,
                Some(&mut returned_bytes),
            )
        };
        if status.is_ok() {
            let filled =
                usize::try_from(returned_bytes).map_err(|_| WindowsApiError::ResourceLimit)?;
            if filled == 0 || filled > capacity_bytes {
                return Err(WindowsApiError::QueryFailed);
            }
            buffer.truncate(filled);
            return Ok(buffer);
        }
        if status.0 != STATUS_INFO_LENGTH_MISMATCH {
            return Err(WindowsApiError::QueryFailed);
        }
        if capacity_bytes >= MAX_BYTES {
            return Err(WindowsApiError::ResourceLimit);
        }
        capacity_bytes = (capacity_bytes * 2).min(MAX_BYTES);
    }
    Err(WindowsApiError::ResourceLimit)
}

/// Duplicate one handle of the owner into this process and resolve its name
/// on a sacrificial thread.
///
/// Returns `None` when the handle closed meanwhile, the name budget ran
/// out, or the query failed or timed out — in every such case the caller
/// keeps the entry with `target: None` (mirroring a failed readlink on
/// `/proc/<pid>/fd`), never a fabricated name.
#[cfg(windows)]
fn resolve_handle_name(
    owner: windows::Win32::Foundation::HANDLE,
    current_process: windows::Win32::Foundation::HANDLE,
    handle_value: u16,
    deadline: &std::time::Instant,
) -> Option<String> {
    use std::time::Instant;

    use windows::Wdk::Foundation::{NtQueryObject, OBJECT_INFORMATION_CLASS};
    use windows::Win32::Foundation::{CloseHandle, DUPLICATE_SAME_ACCESS, HANDLE};

    let mut duplicate_handle = HANDLE::default();
    // SAFETY: `owner` is held open for the whole call; the source value came
    // from the kernel snapshot; the target slot is a valid local; zero
    // desired access with DUPLICATE_SAME_ACCESS preserves the source access.
    let status = unsafe {
        NtDuplicateObject(
            owner,
            HANDLE(core::ptr::with_exposed_provenance_mut(usize::from(
                handle_value,
            ))),
            current_process,
            &mut duplicate_handle,
            0,
            0,
            DUPLICATE_SAME_ACCESS.0,
        )
    };
    if !status.is_ok() || duplicate_handle.is_invalid() {
        // Closed between snapshot and duplication: the entry stays with an
        // unresolved target rather than being fabricated.
        return None;
    }
    if Instant::now() >= *deadline {
        // SAFETY: owned by this call and never handed to another thread.
        unsafe {
            let _ = CloseHandle(duplicate_handle);
        }
        return None;
    }
    query_name_with_sacrificial_thread(duplicate_handle, |handle, buffer| {
        // SAFETY: `handle` is a valid duplicate the caller keeps open for
        // the lifetime of the sacrificial worker, and `buffer` is the
        // worker's own slot, sized to match the length parameter.
        unsafe {
            NtQueryObject(
                Some(handle),
                OBJECT_INFORMATION_CLASS(OBJECT_NAME_INFORMATION_CLASS),
                Some(buffer),
                MAX_NAME_UTF16_UNITS as u32 * 2,
                None,
            )
        }
    })
}

// `NtDuplicateObject` declared once at module scope (see the module doc)
// and consumed through this typed wrapper so every call site shares one
// audited declaration.
#[cfg(windows)]
windows::core::link! {
    "ntdll.dll" "system" fn NtDuplicateObject(
        sourceprocesshandle: windows::Win32::Foundation::HANDLE,
        sourcehandle: windows::Win32::Foundation::HANDLE,
        targetprocesshandle: windows::Win32::Foundation::HANDLE,
        targethandle: *mut windows::Win32::Foundation::HANDLE,
        desiredaccess: u32,
        handleattributes: u32,
        options: u32,
    ) -> windows::Win32::Foundation::NTSTATUS
}

/// Run one `NtQueryObject(ObjectNameInformation)` on a dedicated worker
/// thread. The caller waits a bounded time, then terminates the worker on
/// expiry — the documented-only mitigation for the named-pipe deadlock. The
/// duplicated handle is closed here on every path where the worker is
/// confirmed dead, and deliberately leaked together with the slot when the
/// worker stays wedged in kernel mode (freeing memory a live worker may
/// still write would trade a bounded, rare leak for memory unsafety).
#[cfg(windows)]
fn query_name_with_sacrificial_thread<F>(
    handle: windows::Win32::Foundation::HANDLE,
    query_name: F,
) -> Option<String>
where
    F: Fn(
            windows::Win32::Foundation::HANDLE,
            *mut core::ffi::c_void,
        ) -> windows::Win32::Foundation::NTSTATUS
        + Send
        + 'static,
{
    use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
    use windows::Win32::System::Threading::{
        CreateThread, THREAD_CREATION_FLAGS, TerminateThread, WaitForSingleObject,
    };

    /// Thread parameter: the handle to query, the typed query thunk, and
    /// the shared result slot.
    struct NameQueryJob<F> {
        handle: windows::Win32::Foundation::HANDLE,
        query: F,
        slot: NameQuerySlot,
    }

    // SAFETY: the function uses the Windows thread-entry ABI and receives the
    // exact `Box::into_raw` job pointer supplied to `CreateThread`; the
    // worker validates ownership and publishes its result before returning.
    unsafe extern "system" fn threadproc<F>(parameter: *mut core::ffi::c_void) -> u32
    where
        F: Fn(
                windows::Win32::Foundation::HANDLE,
                *mut core::ffi::c_void,
            ) -> windows::Win32::Foundation::NTSTATUS
            + Send
            + 'static,
    {
        // SAFETY: the parameter is the `Box::into_raw` pointer handed to
        // `CreateThread` below; until this thread signals, only this thread
        // touches the allocation.
        let job = unsafe { &mut *parameter.cast::<NameQueryJob<F>>() };
        // The thunk itself is safe; its own unsafe edge is documented at the
        // definition in `resolve_handle_name`.
        let status = (job.query)(
            job.handle,
            job.slot.name.as_mut_ptr().cast::<core::ffi::c_void>(),
        );
        job.slot.status = status.0;
        if status.is_ok() {
            let buffer_addr = job.slot.name.as_ptr() as usize;
            if let Some(units) = inline_name_units(buffer_addr, &job.slot.name) {
                let header_units = POINTER_BYTES;
                // SAFETY: `inline_name_units` validated both the inline
                // position and the length against the slot buffer.
                unsafe {
                    core::ptr::copy_nonoverlapping(
                        job.slot.name.as_ptr().add(header_units),
                        job.slot.name.as_mut_ptr(),
                        units,
                    );
                }
                job.slot.name_units = units;
            }
        }
        0
    }

    /// RAII guard for the worker thread handle.
    struct ThreadGuard(windows::Win32::Foundation::HANDLE);
    impl Drop for ThreadGuard {
        fn drop(&mut self) {
            // SAFETY: the handle was returned by `CreateThread` and is
            // owned exclusively by this guard; Drop runs at most once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    let job = Box::new(NameQueryJob {
        handle,
        query: query_name,
        slot: NameQuerySlot {
            status: NAME_STATUS_PENDING,
            name_units: 0,
            name: [0; MAX_NAME_UTF16_UNITS],
        },
    });
    let job_pointer = Box::into_raw(job);
    let thread = {
        // SAFETY: `threadproc` matches `LPTHREAD_START_ROUTINE`, the
        // parameter stays valid until the worker is joined or declared
        // wedged below, and default creation flags apply.
        match unsafe {
            CreateThread(
                None,
                0,
                Some(threadproc::<F>),
                Some(job_pointer.cast::<core::ffi::c_void>()),
                THREAD_CREATION_FLAGS(0),
                None,
            )
        } {
            Ok(thread) if !thread.is_invalid() => ThreadGuard(thread),
            _ => {
                // SAFETY: the worker never started, so the job is
                // exclusively ours again.
                drop(unsafe { Box::from_raw(job_pointer) });
                // SAFETY: the duplicate was never handed to a thread.
                unsafe {
                    let _ = CloseHandle(handle);
                }
                return None;
            }
        }
    };

    // SAFETY: `thread` is an owned, valid thread handle.
    let waited = unsafe { WaitForSingleObject(thread.0, NAME_QUERY_TIMEOUT_MS) };
    if waited == WAIT_OBJECT_0 {
        // SAFETY: the worker is finished; the job is exclusively ours again.
        let job = unsafe { Box::from_raw(job_pointer) };
        // SAFETY: the worker is finished with the duplicate.
        unsafe {
            let _ = CloseHandle(handle);
        }
        return decode_slot_name(&job.slot);
    }

    // The documented named-pipe mitigation: terminate the worker, then
    // require it to signal within the grace window.
    // SAFETY: `thread` is an owned, valid thread handle.
    let _ = unsafe { TerminateThread(thread.0, 1) };
    // SAFETY: `thread` is an owned, valid thread handle.
    let waited = unsafe { WaitForSingleObject(thread.0, NAME_QUERY_TERMINATION_GRACE_MS) };
    let dead = waited == WAIT_OBJECT_0;
    if dead {
        // SAFETY: the worker is confirmed dead; the job is ours again.
        let job = unsafe { Box::from_raw(job_pointer) };
        // SAFETY: the worker is confirmed dead.
        unsafe {
            let _ = CloseHandle(handle);
        }
        decode_slot_name(&job.slot)
    } else {
        // Wedged in kernel mode: deliberately leak the job and the
        // duplicated handle. A pending status keeps the result `None`.
        None
    }
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_open_files.rs"]
mod tests;
