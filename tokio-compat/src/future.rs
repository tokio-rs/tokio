#[derive(Debug)]
#[must_use = "futures do nothing unless you `.await` or poll them"]
pub struct Compat<T> {
    inner: T,
}
