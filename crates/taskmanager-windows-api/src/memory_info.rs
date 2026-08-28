//! Compressed-memory store size via the kernel process snapshot.
//!
//! The "Memory Compression" store is a minimal process: it is hidden from
//! ToolHelp32 snapshots (so `sysinfo`-style enumeration never sees it) and
//! handle-based queries need elevation, but `NtQuerySystemInformation(
//! SystemProcessInformation)` — one of the few officially documented
//! classes — returns it with its `WorkingSetSize` to a standard user. That
//! working set is the "In use (compressed)" figure Task Manager shows and is
//! the honest Windows counterpart of the Linux compressed-memory fact.
//!
//! `None` means compression is absent or disabled on this system (e.g.
//! server SKUs) — a real absence, never a zero.

use crate::WindowsApiError;

/// Initial `SystemProcessInformation` snapshot capacity for the growth loop.
#[cfg(windows)]
const INITIAL_SNAPSHOT_BYTES: usize = 128 * 1024;

/// Hard ceiling for one kernel process snapshot: a host whose process table
/// outgrows this is beyond the product's rendering shape, and the read stays
/// bounded instead of chasing an unbounded table.
#[cfg(windows)]
const MAX_SNAPSHOT_BYTES: usize = 4 * 1024 * 1024;

/// Doubling attempts before the growth loop gives up with `ResourceLimit`
/// (128 KiB → 4 MiB in six queries).
#[cfg(windows)]
const MAX_SNAPSHOT_ATTEMPTS: usize = 6;

/// `STATUS_INFO_LENGTH_MISMATCH` (0xC0000004): the documented retry signal
/// meaning "the process table outgrew the supplied buffer".
#[cfg(windows)]
const STATUS_INFO_LENGTH_MISMATCH: i32 = -1_073_741_820;

/// The compression store's image name in the kernel snapshot, matched
/// exactly (UTF-16, byte length included).
const MEMORY_COMPRESSION_IMAGE_NAME: &str = "Memory Compression";

// ---------------------------------------------------------------------------
// Pure snapshot walk.
//
// The parse below reads the documented `SYSTEM_PROCESS_INFORMATION` layout
// (winternl.h) through explicit byte offsets instead of casting the raw
// kernel buffer to the generated struct: the buffer stays an unaligned
// `u8` allocation, and the walk can be proven off-Windows from fixture
// bytes. Offsets derive from the target pointer size so the layout matches
// whatever the kernel wrote for this process's architecture.
// ---------------------------------------------------------------------------

/// Pointer size backing the record layout (`SIZE_T`/`HANDLE` fields).
const POINTER_BYTES: usize = core::mem::size_of::<usize>();

const fn align_to_pointer(value: usize) -> usize {
    (value + POINTER_BYTES - 1) & !(POINTER_BYTES - 1)
}

const NEXT_ENTRY_OFFSET_FIELD: usize = 0;
/// `ImageName` begins after `NextEntryOffset`, `NumberOfThreads`, and the
/// 48-byte `Reserved1` time fields.
const IMAGE_NAME_FIELD: usize = 8 + 48;
/// `UNICODE_STRING::Length` is a byte count, NUL excluded.
const IMAGE_NAME_LENGTH_FIELD: usize = IMAGE_NAME_FIELD;
/// `UNICODE_STRING::Buffer` sits one pointer in (after the two u16 fields,
/// padded on 64-bit targets); `UNICODE_STRING` occupies two pointers total.
const IMAGE_NAME_BUFFER_FIELD: usize = IMAGE_NAME_FIELD + POINTER_BYTES;
const BASE_PRIORITY_FIELD: usize = IMAGE_NAME_FIELD + 2 * POINTER_BYTES;
const UNIQUE_PROCESS_ID_FIELD: usize = align_to_pointer(BASE_PRIORITY_FIELD + 4);
/// `UniqueProcessId` and `Reserved2` are pointer-sized each.
const HANDLE_COUNT_FIELD: usize = UNIQUE_PROCESS_ID_FIELD + 2 * POINTER_BYTES;
const SESSION_ID_FIELD: usize = HANDLE_COUNT_FIELD + 4;
const RESERVED3_FIELD: usize = align_to_pointer(SESSION_ID_FIELD + 4);
const PEAK_VIRTUAL_SIZE_FIELD: usize = RESERVED3_FIELD + POINTER_BYTES;
const VIRTUAL_SIZE_FIELD: usize = PEAK_VIRTUAL_SIZE_FIELD + POINTER_BYTES;
const RESERVED4_FIELD: usize = VIRTUAL_SIZE_FIELD + POINTER_BYTES;
const PEAK_WORKING_SET_FIELD: usize = align_to_pointer(RESERVED4_FIELD + 4);
const WORKING_SET_SIZE_FIELD: usize = PEAK_WORKING_SET_FIELD + POINTER_BYTES;
/// Bytes required to read one entry's working-set size.
const MIN_ENTRY_BYTES: usize = WORKING_SET_SIZE_FIELD + POINTER_BYTES;

/// Resident bytes held by the OS memory-compression store, or `None` when no
/// "Memory Compression" process exists on this system.
#[cfg(windows)]
#[must_use = "inspect the native compression query result"]
pub fn query_memory_compression_used_bytes() -> Result<Option<u64>, WindowsApiError> {
    use windows::Wdk::System::SystemInformation::{
        NtQuerySystemInformation, SystemProcessInformation,
    };

    let mut capacity_bytes = INITIAL_SNAPSHOT_BYTES;
    for _ in 0..MAX_SNAPSHOT_ATTEMPTS {
        let mut buffer = vec![0_u8; capacity_bytes];
        let mut returned_bytes = 0_u32;
        // SAFETY: `buffer` is a writable, `capacity_bytes`-sized allocation
        // that outlives this synchronous call, its length is passed in the
        // matching parameter, and `returned_bytes` receives the kernel's
        // byte count. The documented `SystemProcessInformation` class opens
        // no handles and retains no caller pointers.
        let status = unsafe {
            NtQuerySystemInformation(
                SystemProcessInformation,
                buffer.as_mut_ptr().cast(),
                u32::try_from(buffer.len()).map_err(|_| WindowsApiError::ResourceLimit)?,
                &mut returned_bytes,
            )
        };
        if status.0 >= 0 {
            let filled =
                usize::try_from(returned_bytes).map_err(|_| WindowsApiError::ResourceLimit)?;
            if filled == 0 || filled > buffer.len() {
                return Err(WindowsApiError::QueryFailed);
            }
            return find_memory_compression_working_set(
                &buffer[..filled],
                buffer.as_ptr() as usize,
            );
        }
        if status.0 != STATUS_INFO_LENGTH_MISMATCH {
            return Err(WindowsApiError::QueryFailed);
        }
        capacity_bytes = capacity_bytes
            .checked_mul(2)
            .filter(|grown| *grown <= MAX_SNAPSHOT_BYTES)
            .ok_or(WindowsApiError::ResourceLimit)?;
    }
    Err(WindowsApiError::ResourceLimit)
}

/// Non-Windows hosts keep the lane dormant with the typed fallback.
#[cfg(not(windows))]
pub fn query_memory_compression_used_bytes() -> Result<Option<u64>, WindowsApiError> {
    Err(WindowsApiError::Unsupported)
}

/// Walk one `SystemProcessInformation` snapshot and return the working-set
/// size of the "Memory Compression" store process, or `None` when the
/// snapshot honestly contains no such process.
///
/// `buffer_base` is the userspace address the snapshot occupied, needed to
/// resolve each entry's in-snapshot `ImageName.Buffer` pointer. The walk is
/// pure over raw bytes so a fixture can prove it off-Windows.
fn find_memory_compression_working_set(
    buffer: &[u8],
    buffer_base: usize,
) -> Result<Option<u64>, WindowsApiError> {
    let mut entry_offset = 0_usize;
    loop {
        // Every hop advances by at least one complete entry header, so the
        // loop is bounded by the buffer itself; a link that leaves the
        // buffer or stops advancing is a corrupt snapshot, not a match.
        if buffer
            .get(entry_offset..)
            .is_none_or(|rest| rest.len() < MIN_ENTRY_BYTES)
        {
            return Err(WindowsApiError::QueryFailed);
        }
        if entry_has_memory_compression_name(buffer, entry_offset, buffer_base)? {
            let working_set = read_snapshot_usize(buffer, entry_offset + WORKING_SET_SIZE_FIELD)?;
            return Ok(Some(working_set as u64));
        }
        let next_offset =
            read_snapshot_u32(buffer, entry_offset + NEXT_ENTRY_OFFSET_FIELD)? as usize;
        if next_offset == 0 {
            return Ok(None);
        }
        if next_offset < MIN_ENTRY_BYTES {
            return Err(WindowsApiError::QueryFailed);
        }
        entry_offset = entry_offset
            .checked_add(next_offset)
            .ok_or(WindowsApiError::QueryFailed)?;
    }
}

/// Exact comparison of one entry's image name against the compression
/// store's name, without decoding (or failing on) unrelated entries: a
/// length mismatch resolves to `false` before any pointer is trusted.
fn entry_has_memory_compression_name(
    buffer: &[u8],
    entry_offset: usize,
    buffer_base: usize,
) -> Result<bool, WindowsApiError> {
    let length_bytes = usize::from(read_snapshot_u16(
        buffer,
        entry_offset + IMAGE_NAME_LENGTH_FIELD,
    )?);
    if length_bytes != MEMORY_COMPRESSION_IMAGE_NAME.len() * 2 {
        return Ok(false);
    }
    // The kernel stores image names inside the same snapshot blob, so a
    // `Buffer` outside it cannot be resolved honestly — a corrupt snapshot.
    let name_offset = read_snapshot_usize(buffer, entry_offset + IMAGE_NAME_BUFFER_FIELD)?
        .checked_sub(buffer_base)
        .filter(|offset| {
            offset
                .checked_add(length_bytes)
                .is_some_and(|end| end <= buffer.len())
        })
        .ok_or(WindowsApiError::QueryFailed)?;
    let name_bytes = buffer
        .get(name_offset..name_offset + length_bytes)
        .ok_or(WindowsApiError::QueryFailed)?;
    // The length check above guarantees an even byte count, so every unit
    // is a complete UTF-16 code unit.
    let (units, remainder) = name_bytes.as_chunks::<2>();
    debug_assert!(remainder.is_empty());
    Ok(MEMORY_COMPRESSION_IMAGE_NAME
        .encode_utf16()
        .eq(units.iter().map(|unit| u16::from_le_bytes(*unit))))
}

fn read_snapshot_u16(buffer: &[u8], offset: usize) -> Result<u16, WindowsApiError> {
    let bytes = buffer
        .get(offset..offset + 2)
        .ok_or(WindowsApiError::QueryFailed)?;
    Ok(u16::from_le_bytes([bytes[0], bytes[1]]))
}

fn read_snapshot_u32(buffer: &[u8], offset: usize) -> Result<u32, WindowsApiError> {
    let bytes = buffer
        .get(offset..offset + 4)
        .ok_or(WindowsApiError::QueryFailed)?;
    Ok(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
}

fn read_snapshot_usize(buffer: &[u8], offset: usize) -> Result<usize, WindowsApiError> {
    let bytes = buffer
        .get(offset..offset + POINTER_BYTES)
        .ok_or(WindowsApiError::QueryFailed)?;
    let mut raw = [0_u8; core::mem::size_of::<usize>()];
    raw.copy_from_slice(bytes);
    Ok(usize::from_le_bytes(raw))
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_memory_info.rs"]
mod tests;
