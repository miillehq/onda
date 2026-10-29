//! Memory and CPU time of a process and everything it started, so Onda can report what it really
//! uses. Windows Task Manager lists the WebView2 processes Onda starts under "WebView2 Manager"
//! rather than under Onda, and Linux puts WebKit's processes beside it; both are children of
//! Onda's process in the ordinary case, so walking the process tree finds them.
use crate::PlatformError;
use std::{collections::HashMap, time::Duration};

/// What `memory_bytes` measures.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryMeasure {
    /// Windows' private working set: Task Manager's default Memory column.
    PrivateWorkingSet,
    /// Windows' private commit, where the private working set counter is unavailable.
    PrivateCommit,
    /// Linux: private resident pages (Private_Clean + Private_Dirty).
    PrivateResident,
}

#[derive(Clone, Debug)]
pub struct ProcessSample {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    pub memory_bytes: u64,
    /// User plus kernel time since the process started.
    pub cpu_time: Duration,
}

/// `root` and all of its descendants, root first. Processes that exit or cannot be opened while
/// sampling are skipped rather than failing the whole report.
pub fn process_tree(root: u32) -> Result<(Vec<ProcessSample>, MemoryMeasure), PlatformError> {
    let parents = platform::parents()?;
    let pids = descendants(root, &parents);
    let mut measure = platform::DEFAULT_MEASURE;
    let mut samples = Vec::with_capacity(pids.len());
    for pid in pids {
        if let Some((sample, used)) = platform::sample(pid, parents.get(&pid).copied()) {
            // One process on an older Windows build is enough to label the whole report.
            if used != platform::DEFAULT_MEASURE {
                measure = used;
            }
            samples.push(sample);
        }
    }
    Ok((samples, measure))
}

/// Breadth-first from `root`; a parent map from a single snapshot cannot contain cycles that
/// matter, but the visited set keeps PID reuse from looping anyway.
fn descendants(root: u32, parents: &HashMap<u32, u32>) -> Vec<u32> {
    let mut children: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&pid, &parent) in parents {
        if pid != parent {
            children.entry(parent).or_default().push(pid);
        }
    }
    let mut order = vec![root];
    let mut visited = std::collections::HashSet::from([root]);
    let mut index = 0;
    while let Some(&pid) = order.get(index) {
        let mut next = children.get(&pid).cloned().unwrap_or_default();
        next.sort_unstable();
        for child in next {
            if visited.insert(child) {
                order.push(child);
            }
        }
        index += 1;
    }
    order
}

#[cfg(target_os = "linux")]
mod platform {
    use super::*;
    pub const DEFAULT_MEASURE: MemoryMeasure = MemoryMeasure::PrivateResident;

    pub fn parents() -> Result<HashMap<u32, u32>, PlatformError> {
        let entries = std::fs::read_dir("/proc")
            .map_err(|_| PlatformError::TargetUnavailable("process list unavailable".into()))?;
        Ok(entries
            .flatten()
            .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
            .filter_map(|pid| Some((pid, stat(pid)?.0)))
            .collect())
    }

    /// Parent PID and user+kernel clock ticks. The command name may contain spaces or
    /// parentheses, so fields are read after the last ')'.
    fn stat(pid: u32) -> Option<(u32, u64)> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
        let fields: Vec<&str> = text.get(text.rfind(')')? + 2..)?.split(' ').collect();
        let parent = fields.get(1)?.parse().ok()?;
        let user: u64 = fields.get(11)?.parse().ok()?;
        let kernel: u64 = fields.get(12)?.parse().ok()?;
        Some((parent, user + kernel))
    }

    fn private_resident(pid: u32) -> Option<u64> {
        let text = std::fs::read_to_string(format!("/proc/{pid}/smaps_rollup")).ok()?;
        let kilobytes: u64 = text
            .lines()
            .filter(|line| line.starts_with("Private_Clean:") || line.starts_with("Private_Dirty:"))
            .filter_map(|line| line.split_whitespace().nth(1)?.parse::<u64>().ok())
            .sum();
        Some(kilobytes * 1024)
    }

    pub fn sample(pid: u32, parent: Option<u32>) -> Option<(ProcessSample, MemoryMeasure)> {
        let (_, ticks) = stat(pid)?;
        // SAFETY: sysconf has no preconditions.
        let per_second = unsafe { libc::sysconf(libc::_SC_CLK_TCK) }.max(1) as u64;
        let name = std::fs::read_to_string(format!("/proc/{pid}/comm"))
            .map(|name| name.trim().to_owned())
            .unwrap_or_default();
        Some((
            ProcessSample {
                pid,
                parent: parent.unwrap_or(0),
                name,
                memory_bytes: private_resident(pid).unwrap_or(0),
                cpu_time: Duration::from_millis(ticks * 1000 / per_second),
            },
            DEFAULT_MEASURE,
        ))
    }
}

#[cfg(windows)]
mod platform {
    use super::*;
    use windows_sys::Win32::{
        Foundation::{CloseHandle, FILETIME, INVALID_HANDLE_VALUE},
        System::{
            Diagnostics::ToolHelp::{
                CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
                TH32CS_SNAPPROCESS,
            },
            ProcessStatus::{
                GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS, PROCESS_MEMORY_COUNTERS_EX,
                PROCESS_MEMORY_COUNTERS_EX2,
            },
            Threading::{GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION},
        },
    };
    pub const DEFAULT_MEASURE: MemoryMeasure = MemoryMeasure::PrivateWorkingSet;

    struct Handle(windows_sys::Win32::Foundation::HANDLE);
    impl Drop for Handle {
        fn drop(&mut self) {
            // SAFETY: the handle was returned open by the system and is closed exactly once.
            unsafe { CloseHandle(self.0) };
        }
    }

    pub fn parents() -> Result<HashMap<u32, u32>, PlatformError> {
        // SAFETY: a process snapshot has no preconditions; the handle is closed by `Handle`.
        let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
        if snapshot == INVALID_HANDLE_VALUE {
            return Err(PlatformError::TargetUnavailable(
                "process snapshot unavailable".into(),
            ));
        }
        let snapshot = Handle(snapshot);
        let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
        entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut parents = HashMap::new();
        // SAFETY: `entry` is a correctly sized PROCESSENTRY32W that outlives each call.
        let mut more = unsafe { Process32FirstW(snapshot.0, &mut entry) } != 0;
        while more {
            parents.insert(entry.th32ProcessID, entry.th32ParentProcessID);
            more = unsafe { Process32NextW(snapshot.0, &mut entry) } != 0;
        }
        Ok(parents)
    }

    fn filetime(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }

    fn name(pid: u32) -> String {
        use windows_sys::Win32::System::Threading::{
            QueryFullProcessImageNameW, PROCESS_NAME_WIN32,
        };
        // SAFETY: limited query access is enough for the image name; closed by `Handle`.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return String::new();
        }
        let handle = Handle(handle);
        let mut buffer = [0u16; 1024];
        let mut length = buffer.len() as u32;
        // SAFETY: `buffer` holds `length` UTF-16 units and outlives the call.
        let ok = unsafe {
            QueryFullProcessImageNameW(
                handle.0,
                PROCESS_NAME_WIN32,
                buffer.as_mut_ptr(),
                &mut length,
            )
        } != 0;
        if !ok {
            return String::new();
        }
        let path = String::from_utf16_lossy(&buffer[..length as usize]);
        path.rsplit('\\').next().unwrap_or_default().to_owned()
    }

    pub fn sample(pid: u32, parent: Option<u32>) -> Option<(ProcessSample, MemoryMeasure)> {
        // SAFETY: limited query access; the handle is closed by `Handle`.
        let handle = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
        if handle.is_null() {
            return None;
        }
        let handle = Handle(handle);
        let mut counters: PROCESS_MEMORY_COUNTERS_EX2 = unsafe { std::mem::zeroed() };
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX2>() as u32;
        // SAFETY: the EX2 structure begins with PROCESS_MEMORY_COUNTERS and `size` matches it.
        let extended = unsafe {
            GetProcessMemoryInfo(
                handle.0,
                (&mut counters as *mut PROCESS_MEMORY_COUNTERS_EX2)
                    .cast::<PROCESS_MEMORY_COUNTERS>(),
                size,
            )
        } != 0;
        // The private working set needs Windows 10/11 22H2 with the September 2023 update;
        // older builds reject the larger structure or leave the field empty.
        let (memory, measure) = if extended && counters.PrivateWorkingSetSize > 0 {
            (
                counters.PrivateWorkingSetSize as u64,
                MemoryMeasure::PrivateWorkingSet,
            )
        } else {
            let mut basic: PROCESS_MEMORY_COUNTERS_EX = unsafe { std::mem::zeroed() };
            let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32;
            // SAFETY: as above, for the EX structure.
            let ok = unsafe {
                GetProcessMemoryInfo(
                    handle.0,
                    (&mut basic as *mut PROCESS_MEMORY_COUNTERS_EX)
                        .cast::<PROCESS_MEMORY_COUNTERS>(),
                    size,
                )
            } != 0;
            (
                if ok { basic.PrivateUsage as u64 } else { 0 },
                MemoryMeasure::PrivateCommit,
            )
        };
        let zero = FILETIME {
            dwLowDateTime: 0,
            dwHighDateTime: 0,
        };
        let (mut created, mut exited, mut kernel, mut user) = (zero, zero, zero, zero);
        // SAFETY: all four out-parameters are valid FILETIMEs.
        let timed =
            unsafe { GetProcessTimes(handle.0, &mut created, &mut exited, &mut kernel, &mut user) }
                != 0;
        let hundred_nanoseconds = if timed {
            filetime(kernel) + filetime(user)
        } else {
            0
        };
        Some((
            ProcessSample {
                pid,
                parent: parent.unwrap_or(0),
                name: name(pid),
                memory_bytes: memory,
                cpu_time: Duration::from_nanos(hundred_nanoseconds.saturating_mul(100)),
            },
            measure,
        ))
    }
}

#[cfg(not(any(windows, target_os = "linux")))]
mod platform {
    use super::*;
    pub const DEFAULT_MEASURE: MemoryMeasure = MemoryMeasure::PrivateResident;
    pub fn parents() -> Result<HashMap<u32, u32>, PlatformError> {
        Err(PlatformError::UnsupportedPlatform)
    }
    pub fn sample(_: u32, _: Option<u32>) -> Option<(ProcessSample, MemoryMeasure)> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn descendants_are_found_breadth_first_and_reuse_cannot_loop() {
        let parents = HashMap::from([(10, 1), (11, 10), (12, 10), (13, 11), (20, 1)]);
        assert_eq!(descendants(10, &parents), [10, 11, 12, 13]);
        assert_eq!(descendants(99, &parents), [99]);
        // A reused PID can make a stale entry point back at the root.
        let cyclic = HashMap::from([(11, 10), (12, 11), (10, 12)]);
        assert_eq!(descendants(10, &cyclic), [10, 11, 12]);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn this_process_and_a_child_are_measured() {
        let mut child = std::process::Command::new("sleep")
            .arg("5")
            .spawn()
            .unwrap();
        let (samples, measure) = process_tree(std::process::id()).unwrap();
        child.kill().ok();
        child.wait().ok();
        assert_eq!(measure, MemoryMeasure::PrivateResident);
        assert_eq!(samples[0].pid, std::process::id());
        assert!(samples[0].memory_bytes > 0);
        assert!(samples
            .iter()
            .any(|sample| sample.pid == child.id() && sample.name == "sleep"));
    }
}
