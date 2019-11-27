#[derive(Debug)]
pub(crate) struct Track<T> {
    value: T,
}

impl<T> Track<T> {
    pub(crate) fn new(value: T) -> Track<T> {
        Track { value }
    }

    pub(crate) fn get_mut(&mut self) -> &mut T {
        &mut self.value
    }

    pub(crate) fn into_inner(self) -> T {
        self.value
    }
}
