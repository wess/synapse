//! Liveness and termination by process id, for workers whose supervising
//! session is gone. A worker Synapse still owns is stopped through its own
//! handle; this is only for the orphans left behind when a session was killed
//! rather than closed.

/// Whether a process with this id exists.
#[cfg(unix)]
pub fn alive(process: u32) -> bool {
    process != 0 && unsafe { libc::kill(process as i32, 0) } == 0
}

/// Ask a process to stop.
#[cfg(unix)]
pub fn terminate(process: u32) {
    if process != 0 {
        unsafe {
            libc::kill(process as i32, libc::SIGTERM);
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
