//! Per-OS memory measuring and limits for the PDF worker process (PD-168).
//!
//! The shell watches the worker's resident memory and kills it past the limit; the worker watches
//! itself the same way (so a parent that is slow to look cannot be outrun). On top of that Linux
//! gets an address space limit and Windows a job object with a memory limit, which the kernel
//! enforces without anybody looking. Only the system calls that cannot be had from `std` are here,
//! each behind the smallest `unsafe` block.

/// Resident memory of process `pid` in bytes; `None` where the OS offers it only through a limit
/// (Windows) or the call failed.
#[cfg(target_os = "macos")]
pub fn resident_bytes(pid: u32) -> Option<u64> {
    let size = i32::try_from(std::mem::size_of::<libc::proc_taskinfo>()).ok()?;
    // SAFETY: `proc_pidinfo` fills at most `size` bytes of the buffer it is given, which is a
    // zeroed `proc_taskinfo` of exactly that size.
    unsafe {
        let mut info: libc::proc_taskinfo = std::mem::zeroed();
        let got = libc::proc_pidinfo(
            i32::try_from(pid).ok()?,
            libc::PROC_PIDTASKINFO,
            0,
            std::ptr::addr_of_mut!(info).cast(),
            size,
        );
        (got == size).then_some(info.pti_resident_size)
    }
}

#[cfg(target_os = "linux")]
pub fn resident_bytes(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?;
    let kib: u64 = line.trim().trim_end_matches("kB").trim().parse().ok()?;
    kib.checked_mul(1024)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn resident_bytes(_pid: u32) -> Option<u64> {
    None
}

/// Limits the address space of the calling process (the worker, at its start) on Linux; elsewhere
/// the other limits apply. A failure is not fatal: the watchers still run.
#[cfg(target_os = "linux")]
pub fn limit_own_address_space(bytes: u64) {
    let limit = libc::rlimit {
        rlim_cur: bytes as libc::rlim_t,
        rlim_max: bytes as libc::rlim_t,
    };
    // SAFETY: plain system call with a valid pointer to an initialised struct.
    let _ = unsafe { libc::setrlimit(libc::RLIMIT_AS, &limit) };
}

#[cfg(not(target_os = "linux"))]
pub fn limit_own_address_space(_bytes: u64) {}

/// What keeps a spawned worker confined for as long as it lives. Dropping it releases the
/// confinement (on Windows the job object is closed, which ends a worker still running).
pub struct Confinement {
    #[cfg(windows)]
    job: windows_sys::Win32::Foundation::HANDLE,
}

// SAFETY: the job handle is only closed by the owner.
#[cfg(windows)]
unsafe impl Send for Confinement {}

/// Puts a freshly spawned worker under the memory limit the OS enforces itself (Windows: a job
/// object with a per-process memory limit that also ends the worker when the shell goes away).
#[cfg(windows)]
pub fn confine(child: &std::process::Child, memory_limit: usize) -> Confinement {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JobObjectExtendedLimitInformation,
        SetInformationJobObject, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
        JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE, JOB_OBJECT_LIMIT_PROCESS_MEMORY,
    };
    // SAFETY: documented calls with valid arguments; the struct is zeroed, which is its empty state.
    unsafe {
        let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
        if job.is_null() {
            return Confinement { job };
        }
        let mut info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = std::mem::zeroed();
        info.BasicLimitInformation.LimitFlags =
            JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        info.ProcessMemoryLimit = memory_limit;
        let size = std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32;
        let set = SetInformationJobObject(
            job,
            JobObjectExtendedLimitInformation,
            std::ptr::addr_of!(info).cast(),
            size,
        );
        if set == 0 || AssignProcessToJobObject(job, child.as_raw_handle()) == 0 {
            eprintln!("pdf edit: the worker could not be put under a job object");
        }
        Confinement { job }
    }
}

#[cfg(not(windows))]
pub fn confine(_child: &std::process::Child, _memory_limit: usize) -> Confinement {
    Confinement {}
}

#[cfg(windows)]
impl Drop for Confinement {
    fn drop(&mut self) {
        if !self.job.is_null() {
            // SAFETY: the handle was created by `confine` and is closed once.
            unsafe { windows_sys::Win32::Foundation::CloseHandle(self.job) };
        }
    }
}

#[cfg(test)]
mod tests {
    #[test]
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    fn the_resident_size_of_this_process_is_measured() {
        let own = super::resident_bytes(std::process::id()).expect("measurable");
        assert!(own > 1_000_000, "{own}");
        let mut held = vec![1u8; 64 << 20];
        held[0] = held.len() as u8;
        let grown = super::resident_bytes(std::process::id()).unwrap();
        assert!(grown >= own + (32 << 20), "{own} -> {grown}");
        drop(held);
        assert!(super::resident_bytes(u32::MAX - 1).is_none());
    }
}
