//! Bounded disk read-ahead for the first indicator on a new Windows x64 WebView2 profile.
use std::{
    fs::{self, File},
    io::ErrorKind,
    os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle},
    path::{Path, PathBuf},
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

const MAX_IMAGE_BYTES: u64 = 16 * 1024 * 1024;
const MAX_STARTUP_BYTES: u64 = 32 * 1024 * 1024;
const HOST_AND_BROWSER_IMAGES: [&str; 3] = [
    r"EBWebView\x64\EmbeddedBrowserWebView.dll",
    "msedgewebview2.exe",
    "msedge_elf.dll",
];
const ANGLE_IMAGE: &str = "libGLESv2.dll";
const GRAPHICS_IMAGES: [&str; 4] = [
    ANGLE_IMAGE,
    "vk_swiftshader.dll",
    "vulkan-1.dll",
    "d3dcompiler_47.dll",
];

/// Overlap disk read-ahead with native setup on the first background launch. WebView2
/// creates EBWebView inside Tauri's default user-data directory on first use.
pub fn prefetch_webview2_for_new_profile(default_user_data: PathBuf) {
    let _ = std::thread::Builder::new()
        .name("onda-webview-read-ahead".into())
        .spawn(move || {
            if !is_new_webview_profile(&default_user_data) {
                return;
            }
            if let Some(runtime) = registration::machine_runtime() {
                prefetch_images(&runtime, prefetch_mapping);
            }
        });
}

fn is_new_webview_profile(data_directory: &Path) -> bool {
    fs::symlink_metadata(data_directory.join("EBWebView"))
        .is_err_and(|error| error.kind() == ErrorKind::NotFound)
}

/// Submit the images in observed startup order. Newer runtime layouts may integrate ANGLE
/// differently, so its standalone image identifies the layout measured for the graphics set.
fn prefetch_images(runtime: &Path, mut submit: impl FnMut(&File, u32) -> bool) {
    let mut remaining = MAX_STARTUP_BYTES;
    for image in HOST_AND_BROWSER_IMAGES {
        prefetch_file(&runtime.join(image), &mut remaining, &mut submit);
    }
    if remaining > 0
        && fs::symlink_metadata(runtime.join(ANGLE_IMAGE)).is_ok_and(|image| image.is_file())
    {
        for image in GRAPHICS_IMAGES {
            prefetch_file(&runtime.join(image), &mut remaining, &mut submit);
        }
    }
}

fn prefetch_file(
    path: &Path,
    remaining: &mut u64,
    submit: &mut impl FnMut(&File, u32) -> bool,
) -> bool {
    if *remaining == 0 {
        return false;
    }
    let Ok(file) = File::open(path) else {
        return false;
    };
    let Ok(metadata) = file.metadata() else {
        return false;
    };
    let size = metadata.len();
    if !metadata.is_file() || !(1..=MAX_IMAGE_BYTES.min(*remaining)).contains(&size) {
        return false;
    }
    // Windows can satisfy part of a hint even when it reports failure.
    *remaining -= size;
    submit(&file, size as u32)
}

/// Offer a read-only data mapping to the Windows file cache. The OS decides which pages to read.
fn prefetch_mapping(file: &File, size: u32) -> bool {
    let Some(view) = ReadOnlyView::map(file, size) else {
        return false;
    };
    let range = WIN32_MEMORY_RANGE_ENTRY {
        VirtualAddress: view.0.Value,
        NumberOfBytes: size as usize,
    };
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
mod tests;
