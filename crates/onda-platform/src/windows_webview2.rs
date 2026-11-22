//! Best-effort disk read-ahead for the first Windows dictation indicator.
use std::{
    fs::File,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::Path,
    ptr,
};
use windows_sys::Win32::System::{
    Memory::{
        CreateFileMappingW, MapViewOfFile, PrefetchVirtualMemory, UnmapViewOfFile, FILE_MAP_READ,
        MEMORY_MAPPED_VIEW_ADDRESS, PAGE_READONLY, WIN32_MEMORY_RANGE_ENTRY,
    },
    Threading::GetCurrentProcess,
};

mod registration;

/// The x64 Evergreen host DLL in the cold-start trace was under 10 MiB. Skip larger future
/// versions instead of growing startup I/O or evicting an unrelated application's pages.
const MAX_LOADER_BYTES: u64 = 16 * 1024 * 1024;

/// Submit one read-ahead hint while native services start. The worker runs independently of
/// setup, dictation and shutdown. WebView2 creation still determines indicator readiness.
pub fn prefetch_webview2_loader() {
    let _ = std::thread::Builder::new()
        .name("onda-webview-read-ahead".into())
        .spawn(|| {
            if let Some(path) = registration::machine_loader() {
                prefetch_file(&path);
            }
        });
}

/// Offer a read-only data mapping to the Windows file cache. The OS decides which pages to read.
fn prefetch_file(path: &Path) -> bool {
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    let size = metadata.len();
    if !metadata.is_file() || !(1..=MAX_LOADER_BYTES).contains(&size) {
        return false;
    }
    let Some(view) = ReadOnlyView::map(&file, size as u32) else {
        return false;
    };
    let range = WIN32_MEMORY_RANGE_ENTRY {
        VirtualAddress: view.0.Value,
        NumberOfBytes: size as usize,
    };
    // The kernel coalesces reads for missing pages and leaves cached pages out of our working set.
    unsafe { PrefetchVirtualMemory(GetCurrentProcess(), 1, &range, 0) != 0 }
}

struct ReadOnlyView(MEMORY_MAPPED_VIEW_ADDRESS);

impl ReadOnlyView {
    fn map(file: &File, size: u32) -> Option<Self> {
        // Passing the checked byte count to both calls also bounds the mapping if the file grows.
        let mapping = unsafe {
            CreateFileMappingW(
                file.as_raw_handle(),
                ptr::null(),
                PAGE_READONLY,
                0,
                size,
                ptr::null(),
            )
        };
        if mapping.is_null() {
            return None;
        }
        let mapping = unsafe { OwnedHandle::from_raw_handle(mapping) };
        let view =
            unsafe { MapViewOfFile(mapping.as_raw_handle(), FILE_MAP_READ, 0, 0, size as usize) };
        (!view.Value.is_null()).then(|| Self(view))
    }
}

impl Drop for ReadOnlyView {
    fn drop(&mut self) {
        unsafe { UnmapViewOfFile(self.0) };
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::{self, File, OpenOptions};
    use std::os::windows::fs::OpenOptionsExt;

    #[test]
    fn hint_releases_the_read_only_mapping_and_preserves_the_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("loader");
        let original = [0x5a; 8193];
        fs::write(&path, original).unwrap();
        assert!(
            prefetch_file(&path),
            "no cache hint issued for a small regular file"
        );
        assert_eq!(fs::read(&path).unwrap(), original);
        let exclusive = OpenOptions::new()
            .read(true)
            .write(true)
            .share_mode(0)
            .open(&path)
            .expect("read-ahead retained a file handle or mapping");
        exclusive
            .set_len(0)
            .expect("read-ahead retained a mapped view");
    }

    #[test]
    fn hint_skips_an_empty_missing_or_oversized_file() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("loader");
        assert!(!prefetch_file(&path), "hinted a missing file");
        let file = File::create(&path).unwrap();
        assert!(!prefetch_file(&path), "hinted an empty file");
        file.set_len(MAX_LOADER_BYTES + 1).unwrap();
        drop(file);
        assert!(
            !prefetch_file(&path),
            "read-ahead exceeded its one-file byte budget"
        );
    }
}
