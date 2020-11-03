use crate::stream::{Stream, StreamExt};
use std::iter::Extend;
/// Extends a slice from a Stream
/// # Example
/// `rust,no_run`
/// use tokio::stream;
///
/// let buff = vec![-2];
/// let s = stream::iter(vec![0, 2, 4, 6]);
///
/// stream::extend(&mut buff, s).await;
/// assert_eq!(vec![-2, 0, 2, 4, 6], buff);
pub async fn extend<E, S>(buff: &mut E, b: S)
where
    S: Stream,
    E: Extend<S::Item>,
{
    crate::pin!(b);
    while let Some(item) = b.next().await {
        buff.extend(std::iter::once(item))
    }
}
