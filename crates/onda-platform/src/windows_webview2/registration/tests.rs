use super::*;

const PROGRAM_FILES: &str = r"C:\Program Files (x86)";
const CURRENT: &str = "157.0.4341.60";

#[test]
fn selects_only_the_registered_evergreen_version_and_process_architecture() {
    let runtime = Path::new(PROGRAM_FILES)
        .join(r"Microsoft\EdgeWebView\Application")
        .join(CURRENT);
    for architecture in ["x64", "arm64"] {
        assert_eq!(
            registered_loader(Path::new(PROGRAM_FILES), CURRENT, &runtime, architecture),
            Some(
                runtime
                    .join("EBWebView")
                    .join(architecture)
                    .join("EmbeddedBrowserWebView.dll")
            )
        );
    }
    let stale = runtime.with_file_name("157.0.4341.59");
    assert!(
        registered_loader(Path::new(PROGRAM_FILES), CURRENT, &stale, "x64").is_none(),
        "selected a stale runtime"
    );
    let custom = Path::new(r"D:\custom\157.0.4341.60");
    assert!(
        registered_loader(Path::new(PROGRAM_FILES), CURRENT, custom, "x64").is_none(),
        "selected an override location"
    );
}

#[test]
fn refuses_nonversion_components_and_nonlocal_install_roots() {
    let program_files = Path::new(PROGRAM_FILES);
    for version in [
        "",
        "0.0.0.0",
        "1.2.3",
        "1.2.3.4.5",
        "1.2.3.65536",
        "1.2.3.+4",
        "1.2.3.4 beta",
        r"..\custom",
    ] {
        let runtime = program_files
            .join(r"Microsoft\EdgeWebView\Application")
            .join(version);
        assert!(
            registered_loader(program_files, version, &runtime, "x64").is_none(),
            "accepted malformed version {version:?}"
        );
    }
    for root in [
        r"\\server\share",
        r"\\?\C:\Program Files",
        r"relative",
        r"C:\unsafe\..\elsewhere",
    ] {
        let runtime = Path::new(root)
            .join(r"Microsoft\EdgeWebView\Application")
            .join(CURRENT);
        assert!(
            registered_loader(Path::new(root), CURRENT, &runtime, "x64").is_none(),
            "accepted nonlocal or ambiguous root {root}"
        );
    }
}

#[test]
fn an_applicable_policy_or_user_runtime_suppresses_the_machine_hint() {
    let scratch = ScratchRegistry::new();
    let clean = TestKey::create(scratch.key.0, w!("clean"));
    let user = TestKey::create(scratch.key.0, w!("user"));
    let managed = TestKey::create(scratch.key.0, w!("managed"));

    let _usage_key = TestKey::create(user.0, EVERGREEN_STATE);
    assert!(
        uses_default_registration(clean.0, user.0),
        "a usage-only key hid the system runtime"
    );
    let _registration = TestKey::create(user.0, EVERGREEN_CLIENT);
    assert!(
        !uses_default_registration(clean.0, user.0),
        "selected a competing per-user runtime"
    );

    let _policy = TestKey::create(managed.0, POLICY_KEYS[0]);
    assert!(
        !uses_default_registration(managed.0, clean.0),
        "ignored machine WebView2 policy"
    );
    assert!(
        !uses_default_registration(clean.0, managed.0),
        "ignored per-user WebView2 policy"
    );
}

#[test]
fn the_active_loader_requires_a_literal_registered_path() {
    use windows_sys::Win32::System::Registry::{REG_EXPAND_SZ, REG_SZ};

    let scratch = ScratchRegistry::new();
    let programs = PathBuf::from(std::env::var_os("ProgramFiles").unwrap());
    let suffix = Path::new(r"Microsoft\EdgeWebView\Application").join(CURRENT);
    let runtime = programs.join(&suffix);
    let machine = scratch.key.0;
    let directories = TestKey::create(machine, w!(r"SOFTWARE\Microsoft\Windows\CurrentVersion"));
    directories.set_string(w!("ProgramFilesDir"), REG_SZ, programs.as_os_str());
    let client = TestKey::create(machine, EVERGREEN_CLIENT);
    client.set_string(w!("pv"), REG_SZ, CURRENT.as_ref());
    let state = TestKey::create(machine, EVERGREEN_STATE);

    state.set_string(w!("EBWebView"), REG_SZ, runtime.as_os_str());
    assert_eq!(
        registered_machine_loader(machine, "x64"),
        Some(runtime.join(r"EBWebView\x64\EmbeddedBrowserWebView.dll"))
    );
    let expandable = Path::new("%ProgramFiles%").join(suffix);
    state.set_string(w!("EBWebView"), REG_EXPAND_SZ, expandable.as_os_str());
    assert!(
        registered_machine_loader(machine, "x64").is_none(),
        "accepted a REG_EXPAND_SZ path resolving to the registered loader"
    );

    let user = TestKey::create(machine, w!("user"));
    let user_state = TestKey::create(user.0, EVERGREEN_STATE);
    user_state.set_string(w!("EBWebView"), REG_EXPAND_SZ, expandable.as_os_str());
    assert!(
        !uses_default_registration(machine, user.0),
        "ignored an expandable per-user runtime override"
    );
}

struct TestKey(HKEY);

impl TestKey {
    fn set_string(&self, name: PCWSTR, kind: u32, value: &std::ffi::OsStr) {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::System::Registry::RegSetValueExW;
        let encoded: Vec<u16> = value.encode_wide().chain([0]).collect();
        let status = unsafe {
            RegSetValueExW(
                self.0,
                name,
                0,
                kind,
                encoded.as_ptr().cast(),
                size_of_val(&encoded[..]) as u32,
            )
        };
        assert_eq!(status, ERROR_SUCCESS);
    }

    fn create(parent: HKEY, name: PCWSTR) -> Self {
        use windows_sys::Win32::System::Registry::{
            RegCreateKeyExW, KEY_ALL_ACCESS, REG_CREATED_NEW_KEY, REG_OPTION_VOLATILE,
        };
        let mut handle = ptr::null_mut();
        let mut disposition = 0;
        let status = unsafe {
            RegCreateKeyExW(
                parent,
                name,
                0,
                ptr::null(),
                REG_OPTION_VOLATILE,
                KEY_ALL_ACCESS,
                ptr::null(),
                &mut handle,
                &mut disposition,
            )
        };
        assert_eq!(status, ERROR_SUCCESS);
        assert_eq!(disposition, REG_CREATED_NEW_KEY);
        Self(handle)
    }
}

impl Drop for TestKey {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

/// The fake hives are children of a unique volatile key, away from the actual Edge policies.
struct ScratchRegistry {
    key: TestKey,
    name: Vec<u16>,
}

impl ScratchRegistry {
    fn new() -> Self {
        let unique = tempfile::Builder::new()
            .prefix("Onda-WebView2-test-")
            .tempdir()
            .unwrap();
        let name = format!(
            r"Software\{}",
            unique.path().file_name().unwrap().to_str().unwrap()
        );
        let name: Vec<u16> = name.encode_utf16().chain([0]).collect();
        let key = TestKey::create(HKEY_CURRENT_USER, name.as_ptr());
        Self { key, name }
    }
}

impl Drop for ScratchRegistry {
    fn drop(&mut self) {
        use windows_sys::Win32::System::Registry::RegDeleteTreeW;
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, self.name.as_ptr()) };
    }
}
