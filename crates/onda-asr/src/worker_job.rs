//! Kernel-owned child lifetime, including updater exits and abrupt parent termination.
use std::io;
use tokio::process::Child;
use windows::{
    core::PCWSTR,
    Win32::{
        Foundation::{CloseHandle, HANDLE},
        System::JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
    },
};

/// Unnamed, non-inherited job with unique ownership and kernel kill-on-close semantics.
/// The integer holds the owned handle without placing a raw pointer in async futures.
pub(crate) struct WorkerJob {
    owned_handle: usize,
}

impl WorkerJob {
    /// Configure the lifetime policy before any child is launched.
    pub(crate) fn new() -> io::Result<Self> {
        // SAFETY: null name creates a private job; absent security attributes prevent inheritance.
        let handle = unsafe { CreateJobObjectW(None, PCWSTR::null()) }.map_err(job_error)?;
        let job = Self {
            owned_handle: handle.0 as usize,
        };
        let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
        limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        // SAFETY: the owned handle is live and the struct pointer/size match the information class.
        unsafe {
            SetInformationJobObject(
                job.handle(),
                JobObjectExtendedLimitInformation,
                std::ptr::from_ref(&limits).cast(),
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        }
        .map_err(job_error)?;
        Ok(job)
    }

    /// Assign immediately after spawn, before yielding or sending microphone PCM.
    /// Windows 10/11 support nesting inside a launcher or CI runner's existing job.
    pub(crate) fn assign(&self, child: &Child) -> io::Result<()> {
        let process = child
            .raw_handle()
            .ok_or_else(|| io::Error::other("ASR worker process handle is unavailable"))?;
        // SAFETY: Tokio owns the child process handle; this synchronous call borrows it only.
        unsafe { AssignProcessToJobObject(self.handle(), HANDLE(process)) }.map_err(job_error)
    }

    fn handle(&self) -> HANDLE {
        HANDLE(self.owned_handle as *mut std::ffi::c_void)
    }
}

impl Drop for WorkerJob {
    fn drop(&mut self) {
        // SAFETY: unique ownership closes exactly once. Kernel closure also occurs on process exit.
        let _ = unsafe { CloseHandle(self.handle()) };
    }
}

fn job_error(error: windows::core::Error) -> io::Error {
    io::Error::other(format!(
        "ASR worker containment failed ({:#010x})",
        error.code().0
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{path::PathBuf, process::Stdio, time::Duration};
    const WORKER_STARTUP_GRACE: Duration = Duration::from_millis(500);
    const JOB_TERMINATION_DEADLINE: Duration = Duration::from_secs(30);

    #[tokio::test]
    async fn closing_job_kills_owned_worker_even_with_stdin_open() {
        // Like the worker waiting for its header, findstr blocks reading stdin until it closes.
        let system = PathBuf::from(std::env::var_os("SystemRoot").expect("SystemRoot is set"));
        let job = WorkerJob::new().expect("create private job");
        let mut command = tokio::process::Command::new(system.join("System32").join("findstr.exe"));
        command
            .arg("onda")
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .creation_flags(windows::Win32::System::Threading::CREATE_NO_WINDOW.0);
        let mut child = command.spawn().expect("spawn owned worker");
        job.assign(&child).expect("assign private job");
        let open_stdin = child.stdin.take().expect("keep header pipe open");
        tokio::time::sleep(WORKER_STARTUP_GRACE).await;
        assert!(
            child.try_wait().expect("child status").is_none(),
            "worker must be alive and waiting for its header"
        );
        drop(job);
        let _status = tokio::time::timeout(JOB_TERMINATION_DEADLINE, child.wait())
            .await
            .expect("kernel kills child promptly")
            .expect("reap owned worker");
        // Kill-on-close may report exit code zero; the live header pipe proves this was kernel termination.
        drop(open_stdin);
    }
}
