#[cfg(unix)]
use std::os::fd::AsFd;
#[cfg(windows)]
use std::os::windows::io::AsSocket;

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

#[cfg(windows)]
#[allow(unused_variables)]
pub(crate) fn check_socket_for_blocking<S: AsSocket>(s: &S) -> crate::io::Result<()> {
    #[cfg(debug_assertions)]
    {
        let sock = socket2::SockRef::from(s);

        if !sock.nonblocking()? {
            eprintln!("Warning: binding a nonblocking socket, this may be a bug!");
        }
    }

    Ok(())
}
