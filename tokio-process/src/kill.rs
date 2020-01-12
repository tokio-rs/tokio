use std::io;

/// An interface for killing a running process.
pub(crate) trait Kill {
    /// Forcefully kill the process.
    fn kill(&mut self) -> io::Result<()>;
}

impl<'a, T: 'a + Kill> Kill for &'a mut T {
    fn kill(&mut self) -> io::Result<()> {
        (**self).kill()
    }
}
