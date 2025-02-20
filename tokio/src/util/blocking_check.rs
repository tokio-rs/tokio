#[cfg(unix)]
use std::os::fd::AsFd;

#[cfg(unix)]
#[allow(unused_variables)]
pub(crate) fn check_socket_for_blocking<S: AsFd>(s: &S) -> crate::io::Result<()> {
    #[cfg(debug_assertions)]
    {
        let sock = socket2::SockRef::from(s);

        if !sock.nonblocking()? {
            eprintln!("Warning: binding a nonblocking socket, this may be a bug!");
        }
    }

    Ok(())
}

#[cfg(not(unix))]
#[allow(unused_variables)]
pub(crate) fn check_socket_for_blocking<S>(s: &S) -> crate::io::Result<()> {
    // we cannot retrieve the nonblocking status on windows
    // and i dont know how to support wasi yet
    Ok(())
}
