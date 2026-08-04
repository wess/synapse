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

#[cfg(not(unix))]
pub fn alive(_process: u32) -> bool {
    false
}

#[cfg(not(unix))]
pub fn terminate(_process: u32) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn this_process_is_alive_and_a_zero_id_is_never_one() {
        assert!(alive(std::process::id()));
        assert!(!alive(0));
    }
}
