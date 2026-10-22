//! Windows closes the non-inherited job handle on exit, including forced exit.
//! Only children explicitly assigned by this process are terminated.
use anyhow::{Context, Result};
use std::sync::OnceLock;
use windows_sys::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{
        JobObjects::{
            AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
            SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
            JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        },
        Threading::{OpenProcess, PROCESS_SET_QUOTA, PROCESS_TERMINATE},
    },
};

struct Job(HANDLE);
// The kernel handle supports concurrent assignment and is closed only on drop.
unsafe impl Send for Job {}
unsafe impl Sync for Job {}
impl Job {
    fn new() -> Result<Self> {
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            anyhow::ensure!(
                !handle.is_null(),
                "create child supervision job: {}",
                std::io::Error::last_os_error()
            );
            let job = Self(handle);
            let mut limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
            limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            anyhow::ensure!(
                SetInformationJobObject(
                    handle,
                    JobObjectExtendedLimitInformation,
                    &limits as *const _ as *const _,
                    std::mem::size_of_val(&limits) as u32
                ) != 0,
                "configure child supervision: {}",
                std::io::Error::last_os_error()
            );
            Ok(job)
        }
    }
    fn attach(&self, pid: u32) -> Result<()> {
        unsafe {
            let process = OpenProcess(PROCESS_SET_QUOTA | PROCESS_TERMINATE, 0, pid);
            anyhow::ensure!(
                !process.is_null(),
                "open child for supervision: {}",
                std::io::Error::last_os_error()
            );
            let result = AssignProcessToJobObject(self.0, process);
            let error = std::io::Error::last_os_error();
            CloseHandle(process);
            anyhow::ensure!(result != 0, "assign child supervision: {error}");
            Ok(())
        }
    }
}
impl Drop for Job {
    fn drop(&mut self) {
        unsafe {
            CloseHandle(self.0);
        }
    }
}

pub fn attach(pid: u32) -> Result<()> {
    static JOB: OnceLock<Result<Job, String>> = OnceLock::new();
    JOB.get_or_init(|| Job::new().map_err(|e| e.to_string()))
        .as_ref()
        .map_err(|e| anyhow::anyhow!(e.clone()))
        .context("initialize child supervision")?
        .attach(pid)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_job_kills_owned_child_but_not_unassigned_process() {
        use std::os::windows::process::CommandExt;
        let spawn = || {
            std::process::Command::new("ping.exe")
                .args(["-n", "60", "127.0.0.1"])
                .creation_flags(0x08000000)
                .stdout(std::process::Stdio::null())
                .spawn()
                .unwrap()
        };
        let mut owned = spawn();
        let mut other = spawn();
        let job = Job::new().unwrap();
        job.attach(owned.id()).unwrap();
        drop(job);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while owned.try_wait().unwrap().is_none() && std::time::Instant::now() < deadline {
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let stopped = owned.try_wait().unwrap().is_some();
        let untouched = other.try_wait().unwrap().is_none();
        let _ = owned.kill();
        let _ = owned.wait();
        let _ = other.kill();
        let _ = other.wait();
        assert!(stopped, "owned child survived job closure");
        assert!(untouched, "unassigned process was affected");
    }
}
