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

