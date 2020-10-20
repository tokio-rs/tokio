use crate::stream::{Stream, StreamExt};
use std::marker::Unpin;
/// Extends a slice from a Stream
/// # Example
/// `rust,no_run`
/// use tokio::stream;
///
/// let mut b = vec![0, 0, 0, 0];
/// let stream = stream::iter(vec![0, 2, 4, 6]);
///
/// stream::extend(&mut buff, s).await;
/// assert_eq!(vec![0, 2, 4, 6], buff);
pub async fn extend<T, S>(buff: &mut Vec<T>, mut b: S)
where
    S: Stream<Item = T> + Unpin,
{
    while let Some(item) = b.next().await {
        buff.push(item)
    }
}
