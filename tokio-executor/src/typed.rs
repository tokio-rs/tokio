use SpawnError;

/// TODO: DOX
pub trait TypedExecutor<T> {
    /// TODO: DOX
    fn spawn(&mut self, future: T) -> Result<(), SpawnError>;

    /// TODO: DOX
    fn status(&self) -> Result<(), SpawnError> {
        Ok(())
    }
}

impl<E, T> TypedExecutor<T> for Box<E>
where
    E: TypedExecutor<T>,
{
    fn spawn(&mut self, future: T) -> Result<(), SpawnError> {
        (**self).spawn(future)
    }

    fn status(&self) -> Result<(), SpawnError> {
        (**self).status()
    }
}
