//! Liveness and termination by process id, for workers whose supervising
//! session is gone. A worker Synapse still owns is stopped through its own
//! handle; this is only for the orphans left behind when a session was killed
//! rather than closed.

/// The id as `kill` wants it, or nothing when it does not name one process.
///
/// Zero is the caller's own group, and anything past `i32::MAX` wraps negative,
/// which `kill` reads as a group — or, at minus one, as every process this user
/// owns. These ids come back out of a database row, so a damaged one must not
/// be able to turn "stop this worker" into that.
#[cfg(unix)]
fn single(process: u32) -> Option<libc::pid_t> {
    libc::pid_t::try_from(process).ok().filter(|id| *id > 0)
}

/// Whether a process with this id exists and is this user's. One that belongs
/// to somebody else is refused the signal and reads as gone, which is the right
/// answer: everything asked about here was started by this user, so an id now
/// held by another account is an id that was handed on.
#[cfg(unix)]
pub fn alive(process: u32) -> bool {
    single(process).is_some_and(|id| unsafe { libc::kill(id, 0) } == 0)
}

/// Ask a process to stop.
#[cfg(unix)]
pub fn terminate(process: u32) {
    if let Some(id) = single(process) {
        unsafe {
            libc::kill(id, libc::SIGTERM);
        }
    }
}

#[cfg(windows)]
pub fn alive(process: u32) -> bool {
    use windows_sys::Win32::{
        Foundation::{CloseHandle, WAIT_TIMEOUT},
        System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE, WaitForSingleObject},
    };
    if process == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, process);
        if handle.is_null() {
            return false;
        }
        let running = WaitForSingleObject(handle, 0) == WAIT_TIMEOUT;
        CloseHandle(handle);
        running
    }
}

#[cfg(windows)]
pub fn terminate(process: u32) {
    use windows_sys::Win32::{
        Foundation::CloseHandle,
        System::Threading::{OpenProcess, PROCESS_TERMINATE, TerminateProcess},
    };
    if process == 0 {
        return;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, process);
        if !handle.is_null() {
            TerminateProcess(handle, 1);
            CloseHandle(handle);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_alive_and_a_zero_id_is_never_one() {
        assert!(alive(std::process::id()));
        assert!(!alive(0));
    }

    #[cfg(unix)]
    #[test]
    fn an_id_that_would_name_a_group_names_nothing() {
        // As a signed id these are -1 and -2: every process, and a group.
        assert_eq!(single(u32::MAX), None);
        assert_eq!(single(u32::MAX - 1), None);
        assert_eq!(single(0), None);
        assert!(!alive(u32::MAX));
        // Would be `kill(-1, SIGTERM)` if the wrap got through.
        terminate(u32::MAX);
    }

    #[cfg(windows)]
    #[test]
    fn terminating_a_child_changes_its_liveness() {
        let mut child = std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-Command", "Start-Sleep -Seconds 30"])
            .spawn()
            .unwrap();
        assert!(alive(child.id()));
        terminate(child.id());
        child.wait().unwrap();
        assert!(!alive(child.id()));
    }
}
