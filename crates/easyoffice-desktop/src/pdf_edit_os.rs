//! Per-OS memory measuring and limits for the PDF worker process (PD-170).
//!
//! The shell watches the worker's resident memory and kills it past the limit; the worker watches
//! itself the same way (so a parent that is slow to look cannot be outrun). On top of that Linux
//! gets an address space limit and Windows a job object with a memory limit, which the kernel
//! enforces without anybody looking. Only the system calls that cannot be had from `std` are here,
//! each behind the smallest `unsafe` block.

/// Memory held by process `pid` in bytes; `None` when the call failed. macOS: the physical footprint
/// (`ri_phys_footprint`, which counts compressed memory too: resident size alone drops when the
/// system compresses a process that is growing).
#[cfg(target_os = "macos")]
pub fn resident_bytes(pid: u32) -> Option<u64> {
    // SAFETY: `proc_pid_rusage` fills the zeroed `rusage_info_v2` it is given (the buffer argument
    // is that struct's address, typed loosely by the C header).
    unsafe {
        let mut info: libc::rusage_info_v2 = std::mem::zeroed();
        let got = libc::proc_pid_rusage(
            i32::try_from(pid).ok()?,
            libc::RUSAGE_INFO_V2,
            std::ptr::addr_of_mut!(info).cast(),
        );
        (got == 0).then_some(info.ri_phys_footprint)
    }
}

/// Linux: the resident set of `/proc/<pid>/status`.
#[cfg(target_os = "linux")]
pub fn resident_bytes(pid: u32) -> Option<u64> {
    let status = std::fs::read_to_string(format!("/proc/{pid}/status")).ok()?;
    let line = status.lines().find_map(|l| l.strip_prefix("VmRSS:"))?;
    let kib: u64 = line.trim().trim_end_matches("kB").trim().parse().ok()?;
    kib.checked_mul(1024)
}

/// Memory held by a spawned worker.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn child_resident_bytes(child: &std::process::Child) -> Option<u64> {
    resident_bytes(child.id())
}

/// Memory held by this process.
#[cfg(any(target_os = "macos", target_os = "linux"))]
pub fn own_resident_bytes() -> Option<u64> {
    resident_bytes(std::process::id())
}

/// Other systems (not supported by the shell): nothing is measured.
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn child_resident_bytes(_child: &std::process::Child) -> Option<u64> {
    None
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
pub fn own_resident_bytes() -> Option<u64> {
    None
}

/// Windows: the working set of the process handle (`GetProcessMemoryInfo`).
#[cfg(windows)]
fn working_set(process: windows_sys::Win32::Foundation::HANDLE) -> Option<u64> {
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    // SAFETY: the call fills the zeroed counters it is given; `cb` is their size.
    unsafe {
        let mut counters: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        let size = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        counters.cb = size;
        (GetProcessMemoryInfo(process, &mut counters, size) != 0)
            .then_some(counters.WorkingSetSize as u64)
    }
}

#[cfg(windows)]
pub fn child_resident_bytes(child: &std::process::Child) -> Option<u64> {
    use std::os::windows::io::AsRawHandle;
    working_set(child.as_raw_handle())
}

#[cfg(windows)]
pub fn own_resident_bytes() -> Option<u64> {
    // SAFETY: `GetCurrentProcess` returns a pseudo handle that needs no closing.
    working_set(unsafe { windows_sys::Win32::System::Threading::GetCurrentProcess() })
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
/// `Err` when that did not work: the worker must not run unconfined, so the caller ends it.
#[cfg(windows)]
pub fn confine(child: &std::process::Child, memory_limit: usize) -> Result<Confinement, ()> {
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
            return Err(());
        }
        let confinement = Confinement { job };
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
            return Err(());
        }
        Ok(confinement)
    }
}

#[cfg(not(windows))]
pub fn confine(_child: &std::process::Child, _memory_limit: usize) -> Result<Confinement, ()> {
    Ok(Confinement {})
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
        let own = super::own_resident_bytes().expect("measurable");
        assert!(own > 1_000_000, "{own}");
        // That it follows growth is shown by the worker tests, which are killed at a memory limit.
        assert!(super::resident_bytes(u32::MAX - 1).is_none());
    }
}
