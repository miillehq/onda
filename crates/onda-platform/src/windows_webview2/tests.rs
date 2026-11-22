use super::*;
use std::fs::{self, File, OpenOptions};
use std::os::windows::fs::OpenOptionsExt;

fn hint_one_file(path: &Path) -> bool {
    let mut budget = MAX_STARTUP_BYTES;
    prefetch_file(path, &mut budget, &mut prefetch_mapping)
}

#[test]
fn hint_releases_the_read_only_mapping_and_preserves_the_file() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("loader");
    let original = [0x5a; 8193];
    fs::write(&path, original).unwrap();
    assert!(
        hint_one_file(&path),
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
    assert!(!hint_one_file(&path), "hinted a missing file");
    let file = File::create(&path).unwrap();
    assert!(!hint_one_file(&path), "hinted an empty file");
    file.set_len(MAX_IMAGE_BYTES + 1).unwrap();
    drop(file);
    assert!(
        !hint_one_file(&path),
        "read-ahead exceeded its one-file byte budget"
    );
}

const EXPECTED_IMAGE_PATHS: [&str; 7] = [
    r"EBWebView\x64\EmbeddedBrowserWebView.dll",
    "msedgewebview2.exe",
    "msedge_elf.dll",
    "libGLESv2.dll",
    "vk_swiftshader.dll",
    "vulkan-1.dll",
    "d3dcompiler_47.dll",
];

#[test]
fn a_reserved_parent_is_fresh_and_existing_or_unreadable_profiles_are_skipped() {
    let reserved = tempfile::tempdir().unwrap();
    assert!(is_new_webview_profile(reserved.path()));
    let future_parent = reserved.path().join("after-install");
    assert!(is_new_webview_profile(&future_parent));
    let legacy = reserved.path().join("legacy");
    fs::create_dir_all(legacy.join("EBWebView")).unwrap();
    fs::rename(legacy, &future_parent).unwrap();
    assert!(
        !is_new_webview_profile(&future_parent),
        "hinted a migrated WebView2 profile"
    );
    fs::write(reserved.path().join("EBWebView"), b"occupied").unwrap();
    assert!(
        !is_new_webview_profile(reserved.path()),
        "treated an occupied profile name as fresh"
    );
    let invalid = reserved.path().join("invalid\0path");
    assert!(
        !is_new_webview_profile(&invalid),
        "treated an unreadable profile path as fresh"
    );
}

fn write_image(runtime: &Path, index: usize, length: u64) {
    use std::io::Write;
    let path = runtime.join(EXPECTED_IMAGE_PATHS[index]);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut file = File::create(path).unwrap();
    file.write_all(&[index as u8]).unwrap();
    file.set_len(length).unwrap();
}

fn submitted_images(runtime: &Path, accept: bool) -> Vec<(u8, u32)> {
    use std::io::Read;
    let mut seen = Vec::new();
    prefetch_images(runtime, |mut file, size| {
        let mut id = [0u8];
        file.read_exact(&mut id).unwrap();
        seen.push((id[0], size));
        accept
    });
    seen
}

#[test]
fn submits_the_seven_measured_runtime_images_in_startup_order() {
    let runtime = tempfile::tempdir().unwrap();
    for index in 0..EXPECTED_IMAGE_PATHS.len() {
        write_image(runtime.path(), index, 1);
    }
    fs::write(runtime.path().join("msedge.dll"), [99]).unwrap();
    assert_eq!(
        submitted_images(runtime.path(), true),
        [(0, 1), (1, 1), (2, 1), (3, 1), (4, 1), (5, 1), (6, 1)],
        "did not submit the measured runtime image order"
    );
}

#[test]
fn a_runtime_without_standalone_angle_skips_all_graphics_images() {
    let runtime = tempfile::tempdir().unwrap();
    for index in [0, 1, 2, 4, 5, 6] {
        write_image(runtime.path(), index, 1);
    }
    write_image(&runtime.path().join("another-version"), 3, 1);
    assert_eq!(
        submitted_images(runtime.path(), true),
        [(0, 1), (1, 1), (2, 1)],
        "hinted graphics images from a different runtime layout"
    );
}

#[test]
fn failed_hints_consume_the_shared_budget_and_oversized_files_are_skipped() {
    const SIXTEEN_MIB: u64 = 16 * 1024 * 1024;
    let runtime = tempfile::tempdir().unwrap();
    write_image(runtime.path(), 0, SIXTEEN_MIB);
    write_image(runtime.path(), 1, SIXTEEN_MIB + 1);
    write_image(runtime.path(), 2, SIXTEEN_MIB);
    for index in 3..EXPECTED_IMAGE_PATHS.len() {
        write_image(runtime.path(), index, 1);
    }
    let limit = SIXTEEN_MIB as u32;
    assert_eq!(
        submitted_images(runtime.path(), false),
        [(0, limit), (2, limit)],
        "failed hints or oversized files escaped the shared byte budget"
    );
}
