#![cfg(feature = "rt")]

use futures::{Stream, StreamExt};
use futures_core::FusedStream;
use std::collections::HashSet;
use tokio::task::JoinSet;
use tokio_stream::wrappers::JoinSetStream;

struct Invalid;

#[allow(unused)]
trait AmbiguousIfFused<A> {
    fn some_item(&self) {}
}
impl<T: ?Sized> AmbiguousIfFused<()> for T {}
impl<T: ?Sized + FusedStream> AmbiguousIfFused<Invalid> for T {}

#[allow(unreachable_code)]
#[allow(unused_variables)]
const _: fn() = || {
    let stream: JoinSetStream<()> = todo!();
    AmbiguousIfFused::some_item(&stream);
};

#[tokio::test]
async fn size_hint_stream() {
    let set: JoinSet<_> = (0..2).map(|i| async move { i }).collect();
    let mut stream = JoinSetStream::new(set);

    assert_eq!(stream.size_hint(), (2, Some(2)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (1, Some(1)));
    stream.next().await;
    assert_eq!(stream.size_hint(), (0, Some(0)));
}

#[tokio::test]
async fn join_set_as_stream() {
    let set: JoinSet<_> = (0..2).map(|i| async move { i }).collect();
    let stream = JoinSetStream::new(set);

    let values: HashSet<_> = stream.map(|result| result.unwrap()).collect().await;
    assert_eq!(values, HashSet::from([0, 1]));
}

// Cannot run this test when “unwind” is disabled
// since `JoinSet` use it to catch futures that panics.
#[cfg(panic = "unwind")]
#[tokio::test]
async fn join_set_as_stream_panics_with_error() {
    let set: JoinSet<_> = std::iter::once(async move { panic!("boom!") }).collect();
    let mut stream = JoinSetStream::new(set);

    let result = stream.next().await.transpose();
    assert!(matches!(result, Err(e) if e.is_panic()));
}
