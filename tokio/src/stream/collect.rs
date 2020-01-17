use crate::stream::Stream;

use bytes::{Buf, BufMut, Bytes, BytesMut};
use core::future::Future;
use core::mem;
use core::pin::Pin;
use core::task::{Context, Poll};
use pin_project_lite::pin_project;

// Do not export this struct until `FromStream` can be unsealed.
pin_project! {
    /// Future returned by the [`collect`](super::StreamExt::collect) method.
    #[must_use = "streams do nothing unless polled"]
    #[derive(Debug)]
    pub struct Collect<T, U>
    where
        T: Stream,
        U: FromStream<T::Item>,
    {
        #[pin]
        stream: T,
        collection: U::Collection,
    }
}

/// Convert from a [`Stream`](crate::stream::Stream).
///
/// This trait is not intended to be used directly. Instead, call
/// [`StreamExt::collect()`](super::StreamExt::collect).
///
/// # Implementing
///
/// Currently, this trait may not be implemented by third parties. The trait is
/// sealed in order to make changes in the future. Stabilization is pending
/// enhancements to the Rust langague.
pub trait FromStream<T>: sealed::FromStreamPriv<T> {}

impl<T, U> Collect<T, U>
where
    T: Stream,
    U: FromStream<T::Item>,
{
    pub(super) fn new(stream: T) -> Collect<T, U> {
        let (lower, upper) = stream.size_hint();
        let collection = U::initialize(lower, upper);

        Collect { stream, collection }
    }
}

impl<T, U> Future for Collect<T, U>
where
    T: Stream,
    U: FromStream<T::Item>,
{
    type Output = U;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<U> {
        use Poll::Ready;

        loop {
            let mut me = self.as_mut().project();

            let item = match ready!(me.stream.poll_next(cx)) {
                Some(item) => item,
                None => {
                    return Ready(U::finalize(&mut me.collection));
                }
            };

            if !U::extend(&mut me.collection, item) {
                return Ready(U::finalize(&mut me.collection));
            }
        }
    }
}

// ===== FromStream implementations

impl FromStream<()> for () {}

impl sealed::FromStreamPriv<()> for () {
    type Collection = ();

    fn initialize(_lower: usize, _upper: Option<usize>) {}

    fn extend(_collection: &mut (), _item: ()) -> bool {
        true
    }

    fn finalize(_collection: &mut ()) {}
}

impl<T: AsRef<str>> FromStream<T> for String {}

impl<T: AsRef<str>> sealed::FromStreamPriv<T> for String {
    type Collection = String;

    fn initialize(_lower: usize, _upper: Option<usize>) -> String {
        String::new()
    }

    fn extend(collection: &mut String, item: T) -> bool {
        collection.push_str(item.as_ref());
        true
    }

    fn finalize(collection: &mut String) -> String {
        mem::replace(collection, String::new())
    }
}

impl<T> FromStream<T> for Vec<T> {}

impl<T> sealed::FromStreamPriv<T> for Vec<T> {
    type Collection = Vec<T>;

    fn initialize(lower: usize, _upper: Option<usize>) -> Vec<T> {
        Vec::with_capacity(lower)
    }

    fn extend(collection: &mut Vec<T>, item: T) -> bool {
        collection.push(item);
        true
    }

    fn finalize(collection: &mut Vec<T>) -> Vec<T> {
        mem::replace(collection, vec![])
    }
}

impl<T> FromStream<T> for Box<[T]> {}

impl<T> sealed::FromStreamPriv<T> for Box<[T]> {
    type Collection = Vec<T>;

    fn initialize(lower: usize, upper: Option<usize>) -> Vec<T> {
        <Vec<T> as sealed::FromStreamPriv<T>>::initialize(lower, upper)
    }

    fn extend(collection: &mut Vec<T>, item: T) -> bool {
        <Vec<T> as sealed::FromStreamPriv<T>>::extend(collection, item)
    }

    fn finalize(collection: &mut Vec<T>) -> Box<[T]> {
        <Vec<T> as sealed::FromStreamPriv<T>>::finalize(collection).into_boxed_slice()
    }
}

impl<T, U, E> FromStream<Result<T, E>> for Result<U, E> where U: FromStream<T> {}

impl<T, U, E> sealed::FromStreamPriv<Result<T, E>> for Result<U, E>
where
    U: FromStream<T>,
{
    type Collection = Result<U::Collection, E>;

    fn initialize(lower: usize, upper: Option<usize>) -> Result<U::Collection, E> {
        Ok(U::initialize(lower, upper))
    }

    fn extend(collection: &mut Self::Collection, item: Result<T, E>) -> bool {
        assert!(collection.is_ok());
        match item {
            Ok(item) => {
                let collection = collection.as_mut().ok().expect("invalid state");
                U::extend(collection, item)
            }
            Err(err) => {
                *collection = Err(err);
                false
            }
        }
    }

    fn finalize(collection: &mut Self::Collection) -> Result<U, E> {
        if let Ok(collection) = collection.as_mut() {
            Ok(U::finalize(collection))
        } else {
            let res = mem::replace(collection, Ok(U::initialize(0, Some(0))));

            if let Err(err) = res {
                Err(err)
            } else {
                unreachable!();
            }
        }
    }
}

impl<T: Buf> FromStream<T> for Bytes {}

impl<T: Buf> sealed::FromStreamPriv<T> for Bytes {
    type Collection = BytesMut;

    fn initialize(_lower: usize, _upper: Option<usize>) -> BytesMut {
        BytesMut::new()
    }

    fn extend(collection: &mut BytesMut, item: T) -> bool {
        collection.put(item);
        true
    }

    fn finalize(collection: &mut BytesMut) -> Bytes {
        mem::replace(collection, BytesMut::new()).freeze()
    }
}

impl<T: Buf> FromStream<T> for BytesMut {}

impl<T: Buf> sealed::FromStreamPriv<T> for BytesMut {
    type Collection = BytesMut;

    fn initialize(_lower: usize, _upper: Option<usize>) -> BytesMut {
        BytesMut::new()
    }

    fn extend(collection: &mut BytesMut, item: T) -> bool {
        collection.put(item);
        true
    }

    fn finalize(collection: &mut BytesMut) -> BytesMut {
        mem::replace(collection, BytesMut::new())
    }
}

pub(crate) mod sealed {
    #[doc(hidden)]
    pub trait FromStreamPriv<T> {
        /// Intermediate type used during collection process
        type Collection;

        /// Initialize the collection
        fn initialize(lower: usize, upper: Option<usize>) -> Self::Collection;

        /// Extend the collection with the received item
        ///
        /// Return `true` to continue streaming, `false` complete collection.
        fn extend(collection: &mut Self::Collection, item: T) -> bool;

        /// Finalize collection into target type.
        fn finalize(collection: &mut Self::Collection) -> Self;
    }
}
