use std::io;

#[cfg(unix)]
pub(super) fn terminate_worker_group(pid: u32) -> io::Result<()> {
    use std::os::raw::c_int;
    unsafe extern "C" {
        fn kill(pid: c_int, signal: c_int) -> c_int;
    }
    let pid = c_int::try_from(pid).map_err(|_| io::ErrorKind::InvalidInput)?;
    if unsafe { kill(-pid, 9) } == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}

#[cfg(unix)]
pub(super) fn native_kill_and_reap(pid: u32) -> io::Result<()> {
    use std::os::raw::c_int;
    unsafe extern "C" {
        fn kill(pid: c_int, signal: c_int) -> c_int;
        fn waitpid(pid: c_int, status: *mut c_int, options: c_int) -> c_int;
    }
    let pid = c_int::try_from(pid).map_err(|_| io::ErrorKind::InvalidInput)?;
    // SIGKILL is 9 on the supported Unix targets.
    unsafe {
        let _ = kill(-pid, 9);
        let _ = kill(pid, 9);
        let mut status = 0;
        if waitpid(pid, &mut status, 0) == pid {
            Ok(())
        } else {
            Err(io::Error::last_os_error())
        }
    }
}
