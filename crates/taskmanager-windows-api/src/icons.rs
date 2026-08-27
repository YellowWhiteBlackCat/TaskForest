//! Audited native Windows icon extraction from executables into standard 32-bit BMP payloads.

use crate::WindowsApiError;

/// Extract an executable's small (16x16 / 32x32) icon as standard 32-bit RGBA BMP bytes.
#[must_use = "inspect icon extraction result"]
pub fn extract_process_icon_bmp(executable_path: &str) -> Result<Vec<u8>, WindowsApiError> {
    #[cfg(windows)]
    {
        extract_process_icon_bmp_windows(executable_path)
    }
    #[cfg(not(windows))]
    {
        let _ = executable_path;
        Err(WindowsApiError::Unsupported)
    }
}

#[cfg(windows)]
fn extract_process_icon_bmp_windows(executable_path: &str) -> Result<Vec<u8>, WindowsApiError> {
    use std::ffi::c_void;
    use std::mem::size_of;
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAP, BITMAPFILEHEADER, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC,
        DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC, HGDIOBJ,
    };
    use windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_NORMAL;
    use windows::Win32::UI::Shell::{
        SHFILEINFOW, SHGFI_ICON, SHGFI_SMALLICON, SHGFI_USEFILEATTRIBUTES, SHGetFileInfoW,
    };
    use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
    use windows::core::PCWSTR;

    if executable_path.is_empty() || executable_path.len() > 1024 {
        return Err(WindowsApiError::InvalidInput);
    }

    let mut path_u16: Vec<u16> = executable_path.encode_utf16().collect();
    path_u16.push(0);

    let mut shfi = SHFILEINFOW::default();
    // First try SHGetFileInfoW directly on the path.
    // SAFETY: path_u16 is a null-terminated UTF-16 string and shfi is a valid writable struct.
    let res = unsafe {
        SHGetFileInfoW(
            PCWSTR(path_u16.as_ptr()),
            FILE_ATTRIBUTE_NORMAL,
            Some(&mut shfi),
            size_of::<SHFILEINFOW>() as u32,
            SHGFI_ICON | SHGFI_SMALLICON,
        )
    };

    let hicon = if res != 0 && !shfi.hIcon.is_invalid() {
        shfi.hIcon
    } else {
        // Fallback to SHGFI_USEFILEATTRIBUTES
        // SAFETY: path_u16 is null-terminated, shfi buffer is valid.
        let res = unsafe {
            SHGetFileInfoW(
                PCWSTR(path_u16.as_ptr()),
                FILE_ATTRIBUTE_NORMAL,
                Some(&mut shfi),
                size_of::<SHFILEINFOW>() as u32,
                SHGFI_ICON | SHGFI_SMALLICON | SHGFI_USEFILEATTRIBUTES,
            )
        };
        if res != 0 && !shfi.hIcon.is_invalid() {
            shfi.hIcon
        } else {
            return Err(WindowsApiError::QueryFailed);
        }
    };

    struct IconGuard(HICON);
    impl Drop for IconGuard {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: self.0 is a valid HICON returned by SHGetFileInfoW.
                let _ = unsafe { DestroyIcon(self.0) };
            }
        }
    }
    let _icon_guard = IconGuard(hicon);

    let mut icon_info = ICONINFO::default();
    // SAFETY: hicon is valid and icon_info is writable.
    let ok = unsafe { GetIconInfo(hicon, &mut icon_info) }.is_ok();
    if !ok {
        return Err(WindowsApiError::QueryFailed);
    }

    struct BitmapGuard(HBITMAP);
    impl Drop for BitmapGuard {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: self.0 is a valid GDI bitmap handle returned by GetIconInfo.
                let _ = unsafe { DeleteObject(HGDIOBJ(self.0.0)) };
            }
        }
    }
    let _color_guard = BitmapGuard(icon_info.hbmColor);
    let _mask_guard = BitmapGuard(icon_info.hbmMask);

    let color_bmp = if !icon_info.hbmColor.is_invalid() {
        icon_info.hbmColor
    } else {
        icon_info.hbmMask
    };

    let mut bmp = BITMAP::default();
    // SAFETY: color_bmp is a valid HBITMAP and bmp is a writable struct.
    let bytes_read = unsafe {
        GetObjectW(
            HGDIOBJ(color_bmp.0),
            size_of::<BITMAP>() as i32,
            Some(core::ptr::from_mut(&mut bmp).cast::<c_void>()),
        )
    };
    if bytes_read == 0 || bmp.bmWidth <= 0 || bmp.bmHeight <= 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    let width = bmp.bmWidth as u32;
    let height = if icon_info.hbmColor.is_invalid() {
        (bmp.bmHeight / 2) as u32
    } else {
        bmp.bmHeight.unsigned_abs()
    };

    if width == 0 || height == 0 || width > 256 || height > 256 {
        return Err(WindowsApiError::ResourceLimit);
    }

    struct DcGuard(HDC);
    impl Drop for DcGuard {
        fn drop(&mut self) {
            if !self.0.is_invalid() {
                // SAFETY: self.0 is a valid memory DC returned by CreateCompatibleDC.
                let _ = unsafe { DeleteDC(self.0) };
            }
        }
    }

    // SAFETY: CreateCompatibleDC with None creates a memory DC compatible with screen.
    let hdc = unsafe { CreateCompatibleDC(None) };
    if hdc.is_invalid() {
        return Err(WindowsApiError::QueryFailed);
    }
    let _dc_guard = DcGuard(hdc);

    let pixel_count = (width * height) as usize;
    let mut pixels = vec![0u8; pixel_count * 4];

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32), // Top-down
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            biSizeImage: (pixel_count * 4) as u32,
            ..Default::default()
        },
        ..Default::default()
    };

    // SAFETY: GetDIBits reads the 32-bit pixel data into the allocated vector.
    let lines = unsafe {
        GetDIBits(
            hdc,
            color_bmp,
            0,
            height,
            Some(pixels.as_mut_ptr().cast::<c_void>()),
            &mut bmi,
            DIB_RGB_COLORS,
        )
    };
    if lines == 0 {
        return Err(WindowsApiError::QueryFailed);
    }

    // Check if alpha channel has any non-zero value.
    let has_alpha = pixels.as_chunks::<4>().0.iter().any(|chunk| chunk[3] > 0);
    if !has_alpha {
        // If color bitmap has no alpha, extract 1-bit mask and apply transparency
        if !icon_info.hbmMask.is_invalid() {
            let mut mask_pixels = vec![0u8; pixel_count * 4];
            // SAFETY: hdc, mask handle, and mask_pixels buffer are valid.
            let lines = unsafe {
                GetDIBits(
                    hdc,
                    icon_info.hbmMask,
                    0,
                    height,
                    Some(mask_pixels.as_mut_ptr().cast::<c_void>()),
                    &mut bmi,
                    DIB_RGB_COLORS,
                )
            };
            if lines != 0 {
                for (chunk, mask_chunk) in pixels
                    .as_chunks_mut::<4>()
                    .0
                    .iter_mut()
                    .zip(mask_pixels.as_chunks::<4>().0.iter())
                {
                    // Black in mask means opaque (alpha = 255), white means transparent (alpha = 0)
                    chunk[3] = if mask_chunk[0] == 0 { 255 } else { 0 };
                }
            } else {
                for chunk in pixels.as_chunks_mut::<4>().0 {
                    chunk[3] = 255;
                }
            }
        } else {
            for chunk in pixels.as_chunks_mut::<4>().0 {
                chunk[3] = 255;
            }
        }
    }

    // Construct a standard 32-bit uncompressed BMP file.
    let file_header_size = size_of::<BITMAPFILEHEADER>() as u32;
    let info_header_size = size_of::<BITMAPINFOHEADER>() as u32;
    let off_bits = file_header_size + info_header_size;
    let file_size = off_bits + (pixel_count * 4) as u32;

    let mut bmp_data = Vec::with_capacity(file_size as usize);

    // BITMAPFILEHEADER (14 bytes)
    bmp_data.extend_from_slice(&0x4D42_u16.to_le_bytes()); // 'BM'
    bmp_data.extend_from_slice(&file_size.to_le_bytes());
    bmp_data.extend_from_slice(&0_u16.to_le_bytes()); // Reserved1
    bmp_data.extend_from_slice(&0_u16.to_le_bytes()); // Reserved2
    bmp_data.extend_from_slice(&off_bits.to_le_bytes());

    // BITMAPINFOHEADER (40 bytes)
    bmp_data.extend_from_slice(&info_header_size.to_le_bytes());
    bmp_data.extend_from_slice(&(width as i32).to_le_bytes());
    bmp_data.extend_from_slice(&(-(height as i32)).to_le_bytes()); // Top-down
    bmp_data.extend_from_slice(&1_u16.to_le_bytes()); // Planes
    bmp_data.extend_from_slice(&32_u16.to_le_bytes()); // BitCount
    bmp_data.extend_from_slice(&0_u32.to_le_bytes()); // Compression (BI_RGB)
    bmp_data.extend_from_slice(&((pixel_count * 4) as u32).to_le_bytes()); // SizeImage
    bmp_data.extend_from_slice(&2835_i32.to_le_bytes()); // XPelsPerMeter (~72 DPI)
    bmp_data.extend_from_slice(&2835_i32.to_le_bytes()); // YPelsPerMeter
    bmp_data.extend_from_slice(&0_u32.to_le_bytes()); // ClrUsed
    bmp_data.extend_from_slice(&0_u32.to_le_bytes()); // ClrImportant

    // Pixels (BGRA)
    bmp_data.extend_from_slice(&pixels);

    Ok(bmp_data)
}

#[cfg(test)]
#[path = "../tests/headless/windows_api_icons.rs"]
mod tests;
