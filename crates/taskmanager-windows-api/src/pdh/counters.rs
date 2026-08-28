//! Bounded PDH counter-array storage and native-name decoding.

#[cfg(windows)]
use super::*;

#[cfg(windows)]
pub(super) struct PdhCounterItems {
    // PDH writes both the item array and the pointed-to names into one byte
    // buffer. A Vec<u64> gives the first item the alignment required by the
    // repr(C) item type while still leaving PDH the byte-addressable storage
    // it expects.
    storage: Vec<u64>,
    item_count: usize,
    initialized_bytes: usize,
}

#[cfg(windows)]
impl PdhCounterItems {
    pub(super) fn items(
        &self,
    ) -> &[windows::Win32::System::Performance::PDH_FMT_COUNTERVALUE_ITEM_W] {
        // SAFETY: `query_pdh_counter_items` checks that the native item count
        // fits inside `initialized_bytes`, which is within `storage`. The
        // storage is u64-aligned, satisfying the repr(C) item alignment, and
        // PDH initialized the returned item array before this view is made.
        unsafe { std::slice::from_raw_parts(self.storage.as_ptr().cast(), self.item_count) }
    }

    pub(super) fn decode_name(
        &self,
        pointer: windows::core::PWSTR,
        max_units: usize,
    ) -> Option<String> {
        if pointer.0.is_null() || max_units == 0 {
            return None;
        }

        let start = self.storage.as_ptr() as usize;
        let end = start.checked_add(self.initialized_bytes)?;
        let pointer_address = pointer.0 as usize;
        let pointer_end = pointer_address.checked_add(std::mem::size_of::<u16>())?;
        if pointer_address < start
            || pointer_end > end
            || !pointer_address.is_multiple_of(std::mem::align_of::<u16>())
        {
            return None;
        }

        let available_units = (end - pointer_address) / std::mem::size_of::<u16>();
        let units = available_units.min(max_units);
        // SAFETY: the address is aligned, points inside the native buffer, and
        // `units` is bounded by the bytes returned by PDH.
        let wide = unsafe { std::slice::from_raw_parts(pointer.0.cast_const(), units) };
        let nul = wide.iter().position(|&unit| unit == 0)?;
        String::from_utf16(&wide[..nul]).ok()
    }
}

#[cfg(windows)]
pub(super) struct PdhQuery(windows::Win32::System::Performance::PDH_HQUERY);

#[cfg(windows)]
impl Drop for PdhQuery {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            // SAFETY: self.0 is a valid PDH query handle returned by PdhOpenQueryW.
            let _ = unsafe { windows::Win32::System::Performance::PdhCloseQuery(self.0) };
        }
    }
}

#[cfg(windows)]
pub(super) fn query_pdh_counter_items(
    counter: windows::Win32::System::Performance::PDH_HCOUNTER,
    format: windows::Win32::System::Performance::PDH_FMT,
) -> Result<Option<PdhCounterItems>, WindowsApiError> {
    use windows::Win32::System::Performance::{
        PDH_FMT_COUNTERVALUE_ITEM_W, PdhGetFormattedCounterArrayW,
    };

    let mut required_bytes = 0_u32;
    let mut required_items = 0_u32;
    // SAFETY: The sizing call passes valid writable result pointers and a null
    // item buffer, which is the PDH two-call contract.
    let _ = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            format,
            &mut required_bytes,
            &mut required_items,
            None,
        )
    };

    let required_bytes =
        usize::try_from(required_bytes).map_err(|_| WindowsApiError::ResourceLimit)?;
    let required_items =
        usize::try_from(required_items).map_err(|_| WindowsApiError::ResourceLimit)?;
    if required_bytes == 0 || required_items == 0 {
        return Ok(None);
    }
    if required_bytes > MAX_PDH_BUFFER_BYTES || required_items > MAX_PDH_ITEMS {
        return Err(WindowsApiError::ResourceLimit);
    }

    let item_bytes = required_items
        .checked_mul(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>())
        .ok_or(WindowsApiError::ResourceLimit)?;
    if item_bytes > required_bytes {
        return Err(WindowsApiError::QueryFailed);
    }

    let word_count = required_bytes
        .checked_add(std::mem::size_of::<u64>() - 1)
        .ok_or(WindowsApiError::ResourceLimit)?
        / std::mem::size_of::<u64>();
    let storage_bytes = word_count
        .checked_mul(std::mem::size_of::<u64>())
        .ok_or(WindowsApiError::ResourceLimit)?;
    let storage_bytes_u32 =
        u32::try_from(storage_bytes).map_err(|_| WindowsApiError::ResourceLimit)?;
    let mut storage = vec![0_u64; word_count];
    let item_pointer = storage.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
    let mut buffer_bytes = storage_bytes_u32;
    let mut item_count =
        u32::try_from(required_items).map_err(|_| WindowsApiError::ResourceLimit)?;

    // SAFETY: `storage` is non-empty, u64-aligned, and large enough for the
    // native byte buffer. The pointers and counts remain valid for the call.
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            format,
            &mut buffer_bytes,
            &mut item_count,
            Some(item_pointer),
        )
    };
    if status != 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let initialized_bytes =
        usize::try_from(buffer_bytes).map_err(|_| WindowsApiError::ResourceLimit)?;
    let item_count = usize::try_from(item_count).map_err(|_| WindowsApiError::ResourceLimit)?;
    if initialized_bytes > storage_bytes
        || initialized_bytes > MAX_PDH_BUFFER_BYTES
        || item_count > MAX_PDH_ITEMS
    {
        return Err(WindowsApiError::QueryFailed);
    }
    let item_bytes = item_count
        .checked_mul(std::mem::size_of::<PDH_FMT_COUNTERVALUE_ITEM_W>())
        .ok_or(WindowsApiError::ResourceLimit)?;
    if item_bytes > initialized_bytes {
        return Err(WindowsApiError::QueryFailed);
    }

    Ok(Some(PdhCounterItems {
        storage,
        item_count,
        initialized_bytes,
    }))
}
