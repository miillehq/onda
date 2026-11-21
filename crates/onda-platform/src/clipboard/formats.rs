use super::*;

pub(super) struct SavedFormat {
    pub(super) format: u32,
    pub(super) handle: HANDLE,
}

impl Drop for SavedFormat {
    fn drop(&mut self) {
        if self.handle.is_null() {
            return;
        }
        unsafe {
            match self.format {
                BITMAP_FORMAT | PALETTE_FORMAT => {
                    DeleteObject(self.handle);
                }
                ENHANCED_METAFILE_FORMAT => {
                    DeleteEnhMetaFile(self.handle);
                }
                _ => {
                    GlobalFree(self.handle);
                }
            }
        }
    }
}

pub(super) fn snapshot_formats() -> Result<Vec<SavedFormat>, PlatformError> {
    let mut saved = Vec::new();
    let mut format = 0;
    let mut total_bytes: usize = 0;
    loop {
        unsafe {
            SetLastError(0);
        }
        format = unsafe { EnumClipboardFormats(format) };
        if format == 0 {
            if unsafe { GetLastError() } != 0 {
                return Err(PlatformError::Clipboard(
                    "clipboard enumeration failed".into(),
                ));
            }
            break;
        }
        if saved.len() >= MAX_CLIPBOARD_FORMATS || !preservable_format(format) {
            return Err(PlatformError::UnsupportedClipboardFormat(format));
        }
        let source = unsafe { GetClipboardData(format) };
        if source.is_null() {
            // A delayed-render owner that no longer answers: keep every format that can be kept
            // rather than refusing the user's dictation over one unrecoverable format.
            continue;
        }
        let size = format_size(format, source)?;
        total_bytes = total_bytes
            .checked_add(size)
            .ok_or_else(|| PlatformError::Clipboard("clipboard size overflow".into()))?;
        if total_bytes > MAX_CLIPBOARD_BYTES {
            return Err(PlatformError::Clipboard(
                "clipboard exceeds 64 MiB preservation limit".into(),
            ));
        }
        let handle = if format == ENHANCED_METAFILE_FORMAT {
            unsafe { CopyEnhMetaFileW(source, ptr::null()) }
        } else {
            unsafe { OleDuplicateData(source, format as u16, 0) }
        };
        if handle.is_null() {
            continue;
        }
        saved.push(SavedFormat { format, handle });
    }
    Ok(saved)
}

pub(super) fn preservable_format(format: u32) -> bool {
    // Owner-display, private handles, and old metafile-pictures have custom ownership contracts.
    !matches!(format, 0 | 3 | 0x80..=0x8e | 0x200..=0x2ff) && format <= u16::MAX as u32
}

pub(super) fn format_size(format: u32, handle: HANDLE) -> Result<usize, PlatformError> {
    let size = match format {
        BITMAP_FORMAT => {
            let mut bitmap: BITMAP = unsafe { std::mem::zeroed() };
            if unsafe {
                GetObjectW(
                    handle,
                    std::mem::size_of::<BITMAP>() as i32,
                    &mut bitmap as *mut _ as *mut _,
                )
            } == 0
            {
                return Err(PlatformError::UnsupportedClipboardFormat(format));
            }
            (bitmap.bmWidthBytes.unsigned_abs() as usize)
                .checked_mul(bitmap.bmHeight.unsigned_abs() as usize)
                .ok_or_else(|| PlatformError::Clipboard("bitmap size overflow".into()))?
        }
        PALETTE_FORMAT => {
            (unsafe { GetPaletteEntries(handle, 0, 0, ptr::null_mut()) }) as usize
                * std::mem::size_of::<PALETTEENTRY>()
        }
        ENHANCED_METAFILE_FORMAT => {
            (unsafe { GetEnhMetaFileBits(handle, 0, ptr::null_mut()) }) as usize
        }
        _ => unsafe { GlobalSize(handle) },
    };
    if size == 0 || size > MAX_CLIPBOARD_BYTES {
        return Err(PlatformError::UnsupportedClipboardFormat(format));
    }
    Ok(size)
}

pub(super) fn restore_formats(saved: &mut [SavedFormat]) -> Result<(), PlatformError> {
    if unsafe { EmptyClipboard() } == 0 {
        return Err(PlatformError::Clipboard(
            "clipboard restoration clear failed".into(),
        ));
    }
    for format in saved {
        transfer(format)?;
    }
    Ok(())
}

pub(super) fn transfer(format: &mut SavedFormat) -> Result<(), PlatformError> {
    if unsafe { SetClipboardData(format.format, format.handle) }.is_null() {
        return Err(PlatformError::Clipboard(format!(
            "clipboard format {} publication failed",
            format.format
        )));
    }
    format.handle = ptr::null_mut();
    Ok(())
}

pub(super) fn allocate_format(format: u32, bytes: &[u8]) -> Result<SavedFormat, PlatformError> {
    let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len()) };
    if handle.is_null() {
        return Err(PlatformError::Clipboard(
            "clipboard allocation failed".into(),
        ));
    }
    let data = SavedFormat { format, handle };
    let target = unsafe { GlobalLock(handle) };
    if target.is_null() {
        return Err(PlatformError::Clipboard(
            "clipboard allocation lock failed".into(),
        ));
    }
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr(), target.cast(), bytes.len());
        GlobalUnlock(handle);
    }
    Ok(data)
}

pub(super) fn privacy_formats() -> Result<Vec<SavedFormat>, PlatformError> {
    let mut formats = Vec::with_capacity(3);
    for (name, value) in [
        ("ExcludeClipboardContentFromMonitorProcessing", 1u32),
        ("CanIncludeInClipboardHistory", 0),
        ("CanUploadToCloudClipboard", 0),
    ] {
        let name = wide(name);
        let format = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
        if format == 0 {
            return Err(PlatformError::Clipboard(
                "clipboard privacy marker failed".into(),
            ));
        }
        formats.push(allocate_format(format, &value.to_ne_bytes())?);
    }
    Ok(formats)
}

pub(super) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}
