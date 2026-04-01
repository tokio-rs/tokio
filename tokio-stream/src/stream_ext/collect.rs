use crate::Stream;

use core::future::Future;
use core::marker::{PhantomData, PhantomPinned};
use core::mem;
use core::pin::Pin;
use core::task::{ready, Context, Poll};
use pin_project_lite::pin_project;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap, HashMap, HashSet, LinkedList, VecDeque};
use std::hash::Hash;

// Do not export this struct until `FromStream` can be unsealed.
pin_project! {
    /// Future returned by the [`collect`](super::StreamExt::collect) method.
    #[must_use = "futures do nothing unless you `.await` or poll them"]
    #[derive(Debug)]
    pub struct Collect<T, U, C>
    {
        #[pin]
        stream: T,
        collection: C,
        _output: PhantomData<U>,
        // Make this future `!Unpin` for compatibility with async trait methods.
        #[pin]
        _pin: PhantomPinned,
    }
}

/// Convert from a [`Stream`].
///
/// This trait is not intended to be used directly. Instead, call
/// [`StreamExt::collect()`](super::StreamExt::collect).
///
/// # Implementing
///
/// Currently, this trait may not be implemented by third parties. The trait is
/// sealed in order to make changes in the future. Stabilization is pending
/// enhancements to the Rust language.
pub trait FromStream<T>: sealed::FromStreamPriv<T> {}

impl<T, U> Collect<T, U, U::InternalCollection>
where
    T: Stream,
    U: FromStream<T::Item>,
{
    pub(super) fn new(stream: T) -> Collect<T, U, U::InternalCollection> {
        let (lower, upper) = stream.size_hint();
        let collection = U::initialize(sealed::Internal, lower, upper);

        Collect {
            stream,
            collection,
            _output: PhantomData,
            _pin: PhantomPinned,
        }
    }
}

impl<T, U> Future for Collect<T, U, U::InternalCollection>
where
    T: Stream,
    U: FromStream<T::Item>,
{
    type Output = U;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<U> {
        use Poll::Ready;

        loop {
            let me = self.as_mut().project();

            let item = match ready!(me.stream.poll_next(cx)) {
                Some(item) => item,
                None => {
                    return Ready(U::finalize(sealed::Internal, me.collection));
                }
            };

            if !U::extend(sealed::Internal, me.collection, item) {
                return Ready(U::finalize(sealed::Internal, me.collection));
            }
        }
    }
}

// ===== FromStream implementations

impl FromStream<()> for () {}

impl sealed::FromStreamPriv<()> for () {
    type InternalCollection = ();

    fn initialize(_: sealed::Internal, _lower: usize, _upper: Option<usize>) {}

    fn extend(_: sealed::Internal, _collection: &mut (), _item: ()) -> bool {
        true
    }

    fn finalize(_: sealed::Internal, _collection: &mut ()) {}
}

impl<T: AsRef<str>> FromStream<T> for String {}

impl<T: AsRef<str>> sealed::FromStreamPriv<T> for String {
    type InternalCollection = String;

    fn initialize(_: sealed::Internal, _lower: usize, _upper: Option<usize>) -> String {
        String::new()
    }

    fn extend(_: sealed::Internal, collection: &mut String, item: T) -> bool {
        collection.push_str(item.as_ref());
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut String) -> String {
        mem::take(collection)
    }
}

impl<T> FromStream<T> for Vec<T> {}

impl<T> sealed::FromStreamPriv<T> for Vec<T> {
    type InternalCollection = Vec<T>;

    fn initialize(_: sealed::Internal, lower: usize, _upper: Option<usize>) -> Vec<T> {
        Vec::with_capacity(lower)
    }

    fn extend(_: sealed::Internal, collection: &mut Vec<T>, item: T) -> bool {
        collection.push(item);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut Vec<T>) -> Vec<T> {
        mem::take(collection)
    }
}

impl<T> FromStream<T> for VecDeque<T> {}

impl<T> sealed::FromStreamPriv<T> for VecDeque<T> {
    type InternalCollection = VecDeque<T>;

    fn initialize(_: sealed::Internal, lower: usize, _upper: Option<usize>) -> VecDeque<T> {
        VecDeque::with_capacity(lower)
    }

    fn extend(_: sealed::Internal, collection: &mut VecDeque<T>, item: T) -> bool {
        collection.push_back(item);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut VecDeque<T>) -> VecDeque<T> {
        mem::take(collection)
    }
}

impl<T> FromStream<T> for LinkedList<T> {}

impl<T> sealed::FromStreamPriv<T> for LinkedList<T> {
    type InternalCollection = LinkedList<T>;

    fn initialize(_: sealed::Internal, _lower: usize, _upper: Option<usize>) -> LinkedList<T> {
        LinkedList::new()
    }

    fn extend(_: sealed::Internal, collection: &mut LinkedList<T>, item: T) -> bool {
        collection.push_back(item);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut LinkedList<T>) -> LinkedList<T> {
        mem::take(collection)
    }
}

impl<T: Ord> FromStream<T> for BTreeSet<T> {}

impl<T: Ord> sealed::FromStreamPriv<T> for BTreeSet<T> {
    type InternalCollection = BTreeSet<T>;

    fn initialize(_: sealed::Internal, _lower: usize, _upper: Option<usize>) -> BTreeSet<T> {
        BTreeSet::new()
    }

    fn extend(_: sealed::Internal, collection: &mut BTreeSet<T>, item: T) -> bool {
        collection.insert(item);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut BTreeSet<T>) -> BTreeSet<T> {
        mem::take(collection)
    }
}

impl<K: Ord, V> FromStream<(K, V)> for BTreeMap<K, V> {}

impl<K: Ord, V> sealed::FromStreamPriv<(K, V)> for BTreeMap<K, V> {
    type InternalCollection = BTreeMap<K, V>;

    fn initialize(_: sealed::Internal, _lower: usize, _upper: Option<usize>) -> BTreeMap<K, V> {
        BTreeMap::new()
    }

    fn extend(_: sealed::Internal, collection: &mut BTreeMap<K, V>, (key, value): (K, V)) -> bool {
        collection.insert(key, value);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut BTreeMap<K, V>) -> BTreeMap<K, V> {
        mem::take(collection)
    }
}

impl<T: Eq + Hash> FromStream<T> for HashSet<T> {}

impl<T: Eq + Hash> sealed::FromStreamPriv<T> for HashSet<T> {
    type InternalCollection = HashSet<T>;

    fn initialize(_: sealed::Internal, lower: usize, _upper: Option<usize>) -> HashSet<T> {
        HashSet::with_capacity(lower)
    }

    fn extend(_: sealed::Internal, collection: &mut HashSet<T>, item: T) -> bool {
        collection.insert(item);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut HashSet<T>) -> HashSet<T> {
        mem::take(collection)
    }
}

impl<K: Eq + Hash, V> FromStream<(K, V)> for HashMap<K, V> {}

impl<K: Eq + Hash, V> sealed::FromStreamPriv<(K, V)> for HashMap<K, V> {
    type InternalCollection = HashMap<K, V>;

    fn initialize(_: sealed::Internal, lower: usize, _upper: Option<usize>) -> HashMap<K, V> {
        HashMap::with_capacity(lower)
    }

    fn extend(_: sealed::Internal, collection: &mut HashMap<K, V>, (key, value): (K, V)) -> bool {
        collection.insert(key, value);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut HashMap<K, V>) -> HashMap<K, V> {
        mem::take(collection)
    }
}

impl<T: Ord> FromStream<T> for BinaryHeap<T> {}

impl<T: Ord> sealed::FromStreamPriv<T> for BinaryHeap<T> {
    type InternalCollection = BinaryHeap<T>;

    fn initialize(_: sealed::Internal, lower: usize, _upper: Option<usize>) -> BinaryHeap<T> {
        BinaryHeap::with_capacity(lower)
    }

    fn extend(_: sealed::Internal, collection: &mut BinaryHeap<T>, item: T) -> bool {
        collection.push(item);
        true
    }

    fn finalize(_: sealed::Internal, collection: &mut BinaryHeap<T>) -> BinaryHeap<T> {
        mem::take(collection)
    }
}

impl<T> FromStream<T> for Box<[T]> {}

impl<T> sealed::FromStreamPriv<T> for Box<[T]> {
    type InternalCollection = Vec<T>;

    fn initialize(_: sealed::Internal, lower: usize, upper: Option<usize>) -> Vec<T> {
        <Vec<T> as sealed::FromStreamPriv<T>>::initialize(sealed::Internal, lower, upper)
    }

    fn extend(_: sealed::Internal, collection: &mut Vec<T>, item: T) -> bool {
        <Vec<T> as sealed::FromStreamPriv<T>>::extend(sealed::Internal, collection, item)
    }

    fn finalize(_: sealed::Internal, collection: &mut Vec<T>) -> Box<[T]> {
        <Vec<T> as sealed::FromStreamPriv<T>>::finalize(sealed::Internal, collection)
            .into_boxed_slice()
    }
}

impl<T, U, E> FromStream<Result<T, E>> for Result<U, E> where U: FromStream<T> {}

impl<T, U, E> sealed::FromStreamPriv<Result<T, E>> for Result<U, E>
where
    U: FromStream<T>,
{
    type InternalCollection = Result<U::InternalCollection, E>;

    fn initialize(
        _: sealed::Internal,
        lower: usize,
        upper: Option<usize>,
    ) -> Result<U::InternalCollection, E> {
        Ok(U::initialize(sealed::Internal, lower, upper))
    }

    fn extend(
        _: sealed::Internal,
        collection: &mut Self::InternalCollection,
        item: Result<T, E>,
    ) -> bool {
        assert!(collection.is_ok());
        match item {
            Ok(item) => {
                let collection = collection.as_mut().ok().expect("invalid state");
                U::extend(sealed::Internal, collection, item)
            }
            Err(err) => {
                *collection = Err(err);
                false
            }
        }
    }

    fn finalize(_: sealed::Internal, collection: &mut Self::InternalCollection) -> Result<U, E> {
        if let Ok(collection) = collection.as_mut() {
            Ok(U::finalize(sealed::Internal, collection))
        } else {
            let res = mem::replace(collection, Ok(U::initialize(sealed::Internal, 0, Some(0))));

            Err(res.map(drop).unwrap_err())
        }
    }
}

pub(crate) mod sealed {
    #[doc(hidden)]
    pub trait FromStreamPriv<T> {
        /// Intermediate type used during collection process
        ///
        /// The name of this type is internal and cannot be relied upon.
        type InternalCollection;

        /// Initialize the collection
        fn initialize(
            internal: Internal,
            lower: usize,
            upper: Option<usize>,
        ) -> Self::InternalCollection;

        /// Extend the collection with the received item
        ///
        /// Return `true` to continue streaming, `false` complete collection.
        fn extend(internal: Internal, collection: &mut Self::InternalCollection, item: T) -> bool;

        /// Finalize collection into target type.
        fn finalize(internal: Internal, collection: &mut Self::InternalCollection) -> Self;
    }

    #[allow(missing_debug_implementations)]
    pub struct Internal;
}
