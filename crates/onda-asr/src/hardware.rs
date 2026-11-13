//! Read-only hardware evidence; a driver DLL alone never proves inference works.
use serde::{Deserialize, Serialize};

const MAX_INFERENCE_THREADS: usize = 6;
const MIN_AUTO_GPU_VRAM_BYTES: u64 = 512 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionPreference {
    #[default]
    Auto,
    CpuOnly,
    GpuPreferred,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum ExecutionBackend {
    Cpu,
    Vulkan,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GpuInfo {
    pub name: String,
    pub vendor_id: u32,
    pub dedicated_memory_bytes: u64,
    pub shared_memory_bytes: u64,
    pub software_adapter: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HardwareInfo {
    pub logical_cpus: usize,
    pub physical_cpus: usize,
    pub total_memory_bytes: u64,
    pub available_memory_bytes: u64,
    pub simd: Vec<String>,
    pub gpus: Vec<GpuInfo>,
    pub vulkan_loader_present: bool,
    pub cuda_driver_present: bool,
    pub directml_library_present: bool,
}

/// Memory from /proc/meminfo and whether a Vulkan loader is installed. GPU adapters are not
/// enumerated on Linux; the optional Vulkan worker probes them itself.
#[cfg(target_os = "linux")]
fn inspect_linux(hardware: &mut HardwareInfo) {
    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        let field = |name: &str| {
            meminfo
                .lines()
                .find_map(|line| line.strip_prefix(name))
                .and_then(|rest| rest.split_whitespace().next()?.parse::<u64>().ok())
                .map(|kilobytes| kilobytes * 1024)
        };
        hardware.total_memory_bytes = field("MemTotal:").unwrap_or(0);
        hardware.available_memory_bytes = field("MemAvailable:").unwrap_or(0);
    }
    hardware.vulkan_loader_present = [
        "/usr/lib/x86_64-linux-gnu/libvulkan.so.1",
        "/usr/lib64/libvulkan.so.1",
        "/usr/lib/libvulkan.so.1",
    ]
    .iter()
    .any(|path| std::path::Path::new(path).exists());
}

/// Installed memory from `hw.memsize`, and free memory as free pages times the page size. Profile
/// selection uses installed memory, so without this every Mac stepped down to the Fast models.
/// GPU adapters are not enumerated on macOS.
#[cfg(target_os = "macos")]
fn inspect_macos(hardware: &mut HardwareInfo) {
    fn sysctl<T: Copy + Default>(name: &std::ffi::CStr) -> Option<T> {
        let mut value = T::default();
        let mut size = std::mem::size_of::<T>();
        let status = unsafe {
            libc::sysctlbyname(
                name.as_ptr(),
                &mut value as *mut T as *mut libc::c_void,
                &mut size,
                std::ptr::null_mut(),
                0,
            )
        };
        (status == 0 && size == std::mem::size_of::<T>()).then_some(value)
    }
    hardware.total_memory_bytes = sysctl::<u64>(c"hw.memsize").unwrap_or(0);
    let free_pages = sysctl::<u32>(c"vm.page_free_count").unwrap_or(0);
    let page_size = sysctl::<u64>(c"hw.pagesize").unwrap_or(0);
    hardware.available_memory_bytes =
        (u64::from(free_pages) * page_size).min(hardware.total_memory_bytes);
}

impl HardwareInfo {
    /// Inspect CPU/RAM and DXGI adapters without loading optional GPU runtimes.
    pub fn detect() -> Self {
        let mut hardware = Self {
            logical_cpus: num_cpus::get().max(1),
            physical_cpus: num_cpus::get_physical().max(1),
            total_memory_bytes: 0,
            available_memory_bytes: 0,
            simd: detected_simd(),
            gpus: Vec::new(),
            vulkan_loader_present: false,
            cuda_driver_present: false,
            directml_library_present: false,
        };
        #[cfg(windows)]
        inspect_windows(&mut hardware);
        #[cfg(target_os = "linux")]
        inspect_linux(&mut hardware);
        #[cfg(target_os = "macos")]
        inspect_macos(&mut hardware);
        hardware
    }

    /// Every physical core, so a two- or four-core laptop is not left with one or three. The
    /// shortcut helper runs above normal priority and stays responsive while they are busy.
    pub fn inference_threads(&self) -> usize {
        self.physical_cpus.clamp(1, MAX_INFERENCE_THREADS)
    }

    /// The AVX2 worker is compiled for Haswell-class CPUs (2013+): AVX2, FMA, F16C and BMI2.
    pub fn supports_avx2_worker(&self) -> bool {
        ["avx2", "fma", "f16c", "bmi2"]
            .iter()
            .all(|feature| self.simd.iter().any(|detected| detected == feature))
    }

    /// Auto accelerates discrete devices; explicit GPU preference also tries integrated devices.
    pub fn should_probe_vulkan(&self, preference: ExecutionPreference) -> bool {
        if preference == ExecutionPreference::CpuOnly || !self.vulkan_loader_present {
            return false;
        }
        self.gpus.iter().any(|gpu| {
            !gpu.software_adapter
                && (preference == ExecutionPreference::GpuPreferred
                    || gpu.dedicated_memory_bytes >= MIN_AUTO_GPU_VRAM_BYTES)
        })
    }
}

fn detected_simd() -> Vec<String> {
    let mut result = Vec::new();
    #[cfg(target_arch = "x86_64")]
    {
        if std::is_x86_feature_detected!("sse2") {
            result.push("sse2".into());
        }
        if std::is_x86_feature_detected!("avx") {
            result.push("avx".into());
        }
        if std::is_x86_feature_detected!("avx2") {
            result.push("avx2".into());
        }
        if std::is_x86_feature_detected!("fma") {
            result.push("fma".into());
        }
        if std::is_x86_feature_detected!("f16c") {
            result.push("f16c".into());
        }
        if std::is_x86_feature_detected!("bmi2") {
            result.push("bmi2".into());
        }
        if std::is_x86_feature_detected!("avx512f") {
            result.push("avx512f".into());
        }
    }
    #[cfg(target_arch = "aarch64")]
    result.push("neon".into());
    result
}

#[cfg(windows)]
fn inspect_windows(hardware: &mut HardwareInfo) {
    use windows::Win32::{
        Graphics::Dxgi::{CreateDXGIFactory1, IDXGIFactory1, DXGI_ADAPTER_FLAG_SOFTWARE},
        System::SystemInformation::{GetSystemDirectoryW, GlobalMemoryStatusEx, MEMORYSTATUSEX},
    };
    // SAFETY: structs/buffers have API-prescribed lengths and no pointer outlives the call.
    unsafe {
        let mut memory = MEMORYSTATUSEX {
            dwLength: std::mem::size_of::<MEMORYSTATUSEX>() as u32,
            ..Default::default()
        };
        if GlobalMemoryStatusEx(&mut memory).is_ok() {
            hardware.total_memory_bytes = memory.ullTotalPhys;
            hardware.available_memory_bytes = memory.ullAvailPhys;
        }
        if let Ok(factory) = CreateDXGIFactory1::<IDXGIFactory1>() {
            let mut index = 0;
            while let Ok(adapter) = factory.EnumAdapters1(index) {
                index += 1;
                if let Ok(description) = adapter.GetDesc1() {
                    let end = description
                        .Description
                        .iter()
                        .position(|unit| *unit == 0)
                        .unwrap_or(description.Description.len());
                    hardware.gpus.push(GpuInfo {
                        name: String::from_utf16_lossy(&description.Description[..end]),
                        vendor_id: description.VendorId,
                        dedicated_memory_bytes: description.DedicatedVideoMemory as u64,
                        shared_memory_bytes: description.SharedSystemMemory as u64,
                        software_adapter: description.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32
                            != 0,
                    });
                }
            }
        }
        let mut buffer = [0u16; 32768];
        let length = GetSystemDirectoryW(Some(&mut buffer)) as usize;
        if length > 0 && length < buffer.len() {
            let directory = std::path::PathBuf::from(String::from_utf16_lossy(&buffer[..length]));
            hardware.vulkan_loader_present = directory.join("vulkan-1.dll").is_file();
            hardware.cuda_driver_present = directory.join("nvcuda.dll").is_file();
            hardware.directml_library_present = directory.join("DirectML.dll").is_file();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn threads_use_physical_cores_up_to_the_cap() {
        let mut hardware = HardwareInfo::detect();
        hardware.physical_cpus = 1;
        assert_eq!(hardware.inference_threads(), 1);
        hardware.physical_cpus = 2;
        assert_eq!(hardware.inference_threads(), 2);
        hardware.physical_cpus = 4;
        assert_eq!(hardware.inference_threads(), 4);
        hardware.physical_cpus = 64;
        assert_eq!(hardware.inference_threads(), 6);
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn linux_reports_installed_and_available_memory() {
        let hardware = HardwareInfo::detect();
        assert!(hardware.total_memory_bytes > 0);
        assert!(hardware.available_memory_bytes <= hardware.total_memory_bytes);
        // /proc/meminfo counts kB; any machine that runs the tests has more than 512 MiB.
        assert!(
            hardware.total_memory_bytes > 512 * 1024 * 1024,
            "installed memory must be reported in bytes, got {}",
            hardware.total_memory_bytes
        );
        assert!(hardware.available_memory_bytes > 0);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_reports_installed_memory_so_profiles_do_not_step_down() {
        let hardware = HardwareInfo::detect();
        // Any Mac that runs the tests has more than 4 GiB; 0 sent Balanced and Quality to Fast.
        assert!(
            hardware.total_memory_bytes > 4 << 30,
            "installed memory must be reported in bytes, got {}",
            hardware.total_memory_bytes
        );
        assert!(hardware.available_memory_bytes <= hardware.total_memory_bytes);
    }

    #[test]
    fn avx2_worker_needs_avx2_fma_and_f16c() {
        let mut hardware = HardwareInfo::detect();
        hardware.simd = ["sse2", "avx", "avx2", "fma", "f16c", "bmi2"]
            .map(String::from)
            .to_vec();
        assert!(hardware.supports_avx2_worker());
        hardware.simd.retain(|feature| feature != "f16c");
        assert!(!hardware.supports_avx2_worker());
        hardware.simd = vec!["sse2".into()];
        assert!(!hardware.supports_avx2_worker());
    }
    #[test]
    fn optional_acceleration_never_overrides_cpu_only() {
        let mut hardware = HardwareInfo::detect();
        hardware.vulkan_loader_present = true;
        hardware.gpus = vec![GpuInfo {
            name: "integrated".into(),
            vendor_id: 0x8086,
            dedicated_memory_bytes: 0,
            shared_memory_bytes: 8 << 30,
            software_adapter: false,
        }];
        assert!(!hardware.should_probe_vulkan(ExecutionPreference::Auto));
        assert!(!hardware.should_probe_vulkan(ExecutionPreference::CpuOnly));
        assert!(hardware.should_probe_vulkan(ExecutionPreference::GpuPreferred));
        // A discrete GPU that Auto would accelerate: CPU-only must still never probe it.
        hardware.gpus = vec![GpuInfo {
            name: "discrete".into(),
            vendor_id: 0x10de,
            dedicated_memory_bytes: 4 << 30,
            shared_memory_bytes: 8 << 30,
            software_adapter: false,
        }];
        assert!(
            hardware.should_probe_vulkan(ExecutionPreference::Auto),
            "Auto probes a discrete GPU with at least 512 MiB"
        );
        assert!(
            !hardware.should_probe_vulkan(ExecutionPreference::CpuOnly),
            "CPU-only probed the GPU"
        );
        assert!(hardware.should_probe_vulkan(ExecutionPreference::GpuPreferred));
    }
}
