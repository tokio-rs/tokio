//! A queue of delayed elements.
//!
//! See [`DelayQueue`] for more details.
//!
//! [`DelayQueue`]: struct.DelayQueue.html

use clock::now;
use timer::Handle;
use wheel::{self, Wheel};
use {Delay, Error};

use futures::{Future, Poll, Stream};
use slab::Slab;

use std::cmp;
use std::marker::PhantomData;
use std::time::{Duration, Instant};

/// A queue of delayed elements.
///
/// Once an element is inserted into the `DelayQueue`, it is yielded once the
/// specified deadline has been reached.
///
/// # Usage
///
/// Elements are inserted into `DelayQueue` using the [`insert`] or
/// [`insert_at`] methods. A deadline is provided with the item and a [`Key`] is
/// returned. The key is used to remove the entry or to change the deadline at
/// which it should be yielded back.
///
/// Once delays have been configured, the `DelayQueue` is used via its
/// [`Stream`] implementation. [`poll`] is called. If an entry has reached its
/// deadline, it is returned. If not, `Async::NotReady` indicating that the
/// current task will be notified once the deadline has been reached.
///
/// # `Stream` implementation
///
/// Items are retrieved from the queue via [`Stream::poll`]. If no delays have
/// expired, no items are returned. In this case, `NotReady` is returned and the
/// current task is registered to be notified once the next item's delay has
/// expired.
///
/// If no items are in the queue, i.e. `is_empty()` returns `true`, then `poll`
/// returns `Ready(None)`. This indicates that the stream has reached an end.
/// However, if a new item is inserted *after*, `poll` will once again start
/// returning items or `NotReady.
///
/// Items are returned ordered by their expirations. Items that are configured
/// to expire first will be returned first. There are no ordering guarantees
/// for items configured to expire the same instant. Also note that delays are
/// rounded to the closest millisecond.
///
/// # Implementation
///
/// The `DelayQueue` is backed by the same hashed timing wheel implementation as
/// [`Timer`] as such, it offers the same performance benefits. See [`Timer`]
/// for further implementation notes.
///
/// State associated with each entry is stored in a [`slab`]. This allows
/// amortizing the cost of allocation. Space created for expired entries is
/// reused when inserting new entries.
///
/// Capacity can be checked using [`capacity`] and allocated preemptively by using
/// the [`reserve`] method.
///
/// # Usage
///
/// Using `DelayQueue` to manage cache entries.
///
/// ```rust
/// #[macro_use]
/// extern crate futures;
/// extern crate tokio;
/// # type CacheKey = String;
/// # type Value = String;
/// use tokio::timer::{delay_queue, DelayQueue, Error};
/// use futures::{Async, Poll, Stream};
/// use std::collections::HashMap;
/// use std::time::Duration;
///
/// struct Cache {
///     entries: HashMap<CacheKey, (Value, delay_queue::Key)>,
///     expirations: DelayQueue<CacheKey>,
/// }
///
/// const TTL_SECS: u64 = 30;
///
/// impl Cache {
///     fn insert(&mut self, key: CacheKey, value: Value) {
///         let delay = self.expirations
///             .insert(key.clone(), Duration::from_secs(TTL_SECS));
///
///         self.entries.insert(key, (value, delay));
///     }
///
///     fn get(&self, key: &CacheKey) -> Option<&Value> {
///         self.entries.get(key)
///             .map(|&(ref v, _)| v)
///     }
///
///     fn remove(&mut self, key: &CacheKey) {
///         if let Some((_, cache_key)) = self.entries.remove(key) {
///             self.expirations.remove(&cache_key);
///         }
///     }
///
///     fn poll_purge(&mut self) -> Poll<(), Error> {
///         while let Some(entry) = try_ready!(self.expirations.poll()) {
///             self.entries.remove(entry.get_ref());
///         }
///
///         Ok(Async::Ready(()))
///     }
/// }
/// # fn main() {}
/// ```
///
/// [`insert`]: #method.insert
/// [`insert_at`]: #method.insert_at
/// [`Key`]: struct.Key.html
/// [`Stream`]: https://docs.rs/futures/0.1/futures/stream/trait.Stream.html
/// [`poll`]: #method.poll
/// [`Stream::poll`]: #method.poll
/// [`Timer`]: ../struct.Timer.html
/// [`slab`]: https://docs.rs/slab
/// [`capacity`]: #method.capacity
/// [`reserve`]: #method.reserve
#[derive(Debug)]
pub struct DelayQueue<T> {
    /// Handle to the timer driving the `DelayQueue`
    handle: Handle,

    /// Stores data associated with entries
    slab: Slab<Data<T>>,

    /// Lookup structure tracking all delays in the queue
    wheel: Wheel<Stack<T>>,

    /// Delays that were inserted when already expired. These cannot be stored
    /// in the wheel
    expired: Stack<T>,

    /// Delay expiring when the *first* item in the queue expires
    delay: Option<Delay>,

    /// Wheel polling state
    poll: wheel::Poll,

    /// Instant at which the timer starts
    start: Instant,
}

/// An entry in `DelayQueue` that has expired and removed.
///
/// Values are returned by [`DelayQueue::poll`].
///
/// [`DelayQueue::poll`]: struct.DelayQueue.html#method.poll
#[derive(Debug)]
pub struct Expired<T> {
    /// The data stored in the queue
    data: T,

    /// The expiration time
    deadline: Instant,

    /// The key associated with the entry
    key: Key,
}

/// Token to a value stored in a `DelayQueue`.
///
/// Instances of `Key` are returned by [`DelayQueue::insert`]. See [`DelayQueue`]
/// documentation for more details.
///
/// [`DelayQueue`]: struct.DelayQueue.html
/// [`DelayQueue::insert`]: struct.DelayQueue.html#method.insert
#[derive(Debug, Clone)]
pub struct Key {
    index: usize,
}

#[derive(Debug)]
struct Stack<T> {
    /// Head of the stack
    head: Option<usize>,
    _p: PhantomData<T>,
}

#[derive(Debug)]
struct Data<T> {
    /// The data being stored in the queue and will be returned at the requested
    /// instant.
    inner: T,

    /// The instant at which the item is returned.
    when: u64,

    /// Set to true when stored in the `expired` queue
    expired: bool,

    /// Next entry in the stack
    next: Option<usize>,

    /// Previous entry in the stack
    prev: Option<usize>,
}

/// Maximum number of entries the queue can handle
const MAX_ENTRIES: usize = (1 << 30) - 1;

impl<T> DelayQueue<T> {
    /// Create a new, empty, `DelayQueue`
    ///
    /// The queue will not allocate storage until items are inserted into it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio_timer::DelayQueue;
    /// let delay_queue: DelayQueue<u32> = DelayQueue::new();
    /// ```
    pub fn new() -> DelayQueue<T> {
        DelayQueue::with_capacity(0)
    }

    /// Create a new, empty, `DelayQueue` backed by the specified timer.
    ///
    /// The queue will not allocate storage until items are inserted into it.
    ///
    /// # Examples
    ///
    /// ```rust,no_run
    /// # use tokio_timer::DelayQueue;
    /// use tokio_timer::timer::Handle;
    ///
    /// let handle = Handle::default();
    /// let delay_queue: DelayQueue<u32> = DelayQueue::with_capacity_and_handle(0, &handle);
    /// ```
    pub fn with_capacity_and_handle(capacity: usize, handle: &Handle) -> DelayQueue<T> {
        DelayQueue {
            handle: handle.clone(),
            wheel: Wheel::new(),
            slab: Slab::with_capacity(capacity),
            expired: Stack::default(),
            delay: None,
            poll: wheel::Poll::new(0),
            start: now(),
        }
    }

    /// Create a new, empty, `DelayQueue` with the specified capacity.
    ///
    /// The queue will be able to hold at least `capacity` elements without
    /// reallocating. If `capacity` is 0, the queue will not allocate for
    /// storage.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio_timer::DelayQueue;
    /// # use std::time::Duration;
    /// let mut delay_queue = DelayQueue::with_capacity(10);
    ///
    /// // These insertions are done without further allocation
    /// for i in 0..10 {
    ///     delay_queue.insert(i, Duration::from_secs(i));
    /// }
    ///
    /// // This will make the queue allocate additional storage
    /// delay_queue.insert(11, Duration::from_secs(11));
    /// ```
    pub fn with_capacity(capacity: usize) -> DelayQueue<T> {
        DelayQueue::with_capacity_and_handle(capacity, &Handle::default())
    }

    /// Insert `value` into the queue set to expire at a specific instant in
    /// time.
    ///
    /// This function is identical to `insert`, but takes an `Instant` instead
    /// of a `Duration`.
    ///
    /// `value` is stored in the queue until `when` is reached. At which point,
    /// `value` will be returned from [`poll`]. If `when` has already been
    /// reached, then `value` is immediately made available to poll.
    ///
    /// The return value represents the insertion and is used at an argument to
    /// [`remove`] and [`reset`]. Note that [`Key`] is token and is reused once
    /// `value` is removed from the queue either by calling [`poll`] after
    /// `when` is reached or by calling [`remove`]. At this point, the caller
    /// must take care to not use the returned [`Key`] again as it may reference
    /// a different item in the queue.
    ///
    /// See [type] level documentation for more details.
    ///
    /// # Panics
    ///
    /// This function panics if `when` is too far in the future.
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```rust
    /// # extern crate tokio;
    /// use tokio::timer::DelayQueue;
    /// use std::time::{Instant, Duration};
    ///
    /// # fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert_at(
    ///     "foo", Instant::now() + Duration::from_secs(5));
    ///
    /// // Remove the entry
    /// let item = delay_queue.remove(&key);
    /// assert_eq!(*item.get_ref(), "foo");
    /// # }
    /// ```
    ///
    /// [`poll`]: #method.poll
    /// [`remove`]: #method.remove
    /// [`reset`]: #method.reset
    /// [`Key`]: struct.Key.html
    /// [type]: #
    pub fn insert_at(&mut self, value: T, when: Instant) -> Key {
        assert!(self.slab.len() < MAX_ENTRIES, "max entries exceeded");

        // Normalize the deadline. Values cannot be set to expire in the past.
        let when = self.normalize_deadline(when);

        // Insert the value in the store
        let key = self.slab.insert(Data {
            inner: value,
            when,
            expired: false,
            next: None,
            prev: None,
        });

        self.insert_idx(when, key);

        // Set a new delay if the current's deadline is later than the one of the new item
        let should_set_delay = if let Some(ref delay) = self.delay {
            let current_exp = self.normalize_deadline(delay.deadline());
            current_exp > when
        } else {
            true
        };

        if should_set_delay {
            self.delay = Some(self.handle.delay(self.start + Duration::from_millis(when)));
        }

        Key::new(key)
    }

    /// Insert `value` into the queue set to expire after the requested duration
    /// elapses.
    ///
    /// This function is identical to `insert_at`, but takes a `Duration`
    /// instead of an `Instant`.
    ///
    /// `value` is stored in the queue until `when` is reached. At which point,
    /// `value` will be returned from [`poll`]. If `when` has already been
    /// reached, then `value` is immediately made available to poll.
    ///
    /// The return value represents the insertion and is used at an argument to
    /// [`remove`] and [`reset`]. Note that [`Key`] is token and is reused once
    /// `value` is removed from the queue either by calling [`poll`] after
    /// `when` is reached or by calling [`remove`]. At this point, the caller
    /// must take care to not use the returned [`Key`] again as it may reference
    /// a different item in the queue.
    ///
    /// See [type] level documentation for more details.
    ///
    /// # Panics
    ///
    /// This function panics if `timeout` is greater than the maximum supported
    /// duration.
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```rust
    /// # extern crate tokio;
    /// use tokio::timer::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// // Remove the entry
    /// let item = delay_queue.remove(&key);
    /// assert_eq!(*item.get_ref(), "foo");
    /// # }
    /// ```
    ///
    /// [`poll`]: #method.poll
    /// [`remove`]: #method.remove
    /// [`reset`]: #method.reset
    /// [`Key`]: struct.Key.html
    /// [type]: #
    pub fn insert(&mut self, value: T, timeout: Duration) -> Key {
        self.insert_at(value, now() + timeout)
    }

    fn insert_idx(&mut self, when: u64, key: usize) {
        use self::wheel::{InsertError, Stack};

        // Register the deadline with the timer wheel
        match self.wheel.insert(when, key, &mut self.slab) {
            Ok(_) => {}
            Err((_, InsertError::Elapsed)) => {
                self.slab[key].expired = true;
                // The delay is already expired, store it in the expired queue
                self.expired.push(key, &mut self.slab);
            }
            Err((_, err)) => panic!("invalid deadline; err={:?}", err),
        }
    }

    /// Remove the item associated with `key` from the queue.
    ///
    /// There must be an item associated with `key`. The function returns the
    /// removed item as well as the `Instant` at which it will the delay will
    /// have expired.
    ///
    /// # Panics
    ///
    /// The function panics if `key` is not contained by the queue.
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```rust
    /// # extern crate tokio;
    /// use tokio::timer::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// // Remove the entry
    /// let item = delay_queue.remove(&key);
    /// assert_eq!(*item.get_ref(), "foo");
    /// # }
    /// ```
    pub fn remove(&mut self, key: &Key) -> Expired<T> {
        use wheel::Stack;

        // Special case the `expired` queue
        if self.slab[key.index].expired {
            self.expired.remove(&key.index, &mut self.slab);
        } else {
            self.wheel.remove(&key.index, &mut self.slab);
        }

        let data = self.slab.remove(key.index);

        Expired {
            key: Key::new(key.index),
            data: data.inner,
            deadline: self.start + Duration::from_millis(data.when),
        }
    }

    /// Sets the delay of the item associated with `key` to expire at `when`.
    ///
    /// This function is identical to `reset` but takes an `Instant` instead of
    /// a `Duration`.
    ///
    /// The item remains in the queue but the delay is set to expire at `when`.
    /// If `when` is in the past, then the item is immediately made available to
    /// the caller.
    ///
    /// # Panics
    ///
    /// This function panics if `when` is too far in the future or if `key` is
    /// not contained by the queue.
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```rust
    /// # extern crate tokio;
    /// use tokio::timer::DelayQueue;
    /// use std::time::{Duration, Instant};
    ///
    /// # fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// // "foo" is scheduled to be returned in 5 seconds
    ///
    /// delay_queue.reset_at(&key, Instant::now() + Duration::from_secs(10));
    ///
    /// // "foo"is now scheduled to be returned in 10 seconds
    /// # }
    /// ```
    pub fn reset_at(&mut self, key: &Key, when: Instant) {
        self.wheel.remove(&key.index, &mut self.slab);

        // Normalize the deadline. Values cannot be set to expire in the past.
        let when = self.normalize_deadline(when);

        self.slab[key.index].when = when;
        self.insert_idx(when, key.index);

        let next_deadline = self.next_deadline();
        if let (Some(ref mut delay), Some(deadline)) = (&mut self.delay, next_deadline) {
            delay.reset(deadline);
        }
    }

    /// Returns the next time poll as determined by the wheel
    fn next_deadline(&mut self) -> Option<Instant> {
        self.wheel
            .poll_at()
            .map(|poll_at| self.start + Duration::from_millis(poll_at))
    }

    /// Sets the delay of the item associated with `key` to expire after
    /// `timeout`.
    ///
    /// This function is identical to `reset_at` but takes a `Duration` instead
    /// of an `Instant`.
    ///
    /// The item remains in the queue but the delay is set to expire after
    /// `timeout`.  If `timeout` is zero, then the item is immediately made
    /// available to the caller.
    ///
    /// # Panics
    ///
    /// This function panics if `timeout` is greater than the maximum supported
    /// duration or if `key` is not contained by the queue.
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```rust
    /// # extern crate tokio;
    /// use tokio::timer::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// // "foo" is scheduled to be returned in 5 seconds
    ///
    /// delay_queue.reset(&key, Duration::from_secs(10));
    ///
    /// // "foo"is now scheduled to be returned in 10 seconds
    /// # }
    /// ```
    pub fn reset(&mut self, key: &Key, timeout: Duration) {
        self.reset_at(key, now() + timeout);
    }

    /// Clears the queue, removing all items.
    ///
    /// After calling `clear`, [`poll`] will return `Ok(Ready(None))`.
    ///
    /// Note that this method has no effect on the allocated capacity.
    ///
    /// [`poll`]: #method.poll
    ///
    /// # Examples
    ///
    /// ```rust
    /// # extern crate tokio;
    /// use tokio::timer::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # fn main() {
    /// let mut delay_queue = DelayQueue::new();
    ///
    /// delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// assert!(!delay_queue.is_empty());
    ///
    /// delay_queue.clear();
    ///
    /// assert!(delay_queue.is_empty());
    /// # }
    /// ```
    pub fn clear(&mut self) {
        self.slab.clear();
        self.expired = Stack::default();
        self.wheel = Wheel::new();
        self.delay = None;
    }

    /// Returns the number of elements the queue can hold without reallocating.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio_timer::DelayQueue;
    /// let delay_queue: DelayQueue<i32> = DelayQueue::with_capacity(10);
    /// assert_eq!(delay_queue.capacity(), 10);
    /// ```
    pub fn capacity(&self) -> usize {
        self.slab.capacity()
    }

    /// Reserve capacity for at least `additional` more items to be queued
    /// without allocating.
    ///
    /// `reserve` does nothing if the queue already has sufficient capacity for
    /// `additional` more values. If more capacity is required, a new segment of
    /// memory will be allocated and all existing values will be copied into it.
    /// As such, if the queue is already very large, a call to `reserve` can end
    /// up being expensive.
    ///
    /// The queue may reserve more than `additional` extra space in order to
    /// avoid frequent reallocations.
    ///
    /// # Panics
    ///
    /// Panics if the new capacity exceeds the maximum number of entries the
    /// queue can contain.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_timer::DelayQueue;
    /// # use std::time::Duration;
    /// let mut delay_queue = DelayQueue::new();
    /// delay_queue.insert("hello", Duration::from_secs(10));
    /// delay_queue.reserve(10);
    /// assert!(delay_queue.capacity() >= 11);
    /// ```
    pub fn reserve(&mut self, additional: usize) {
        self.slab.reserve(additional);
    }

    /// Returns `true` if there are no items in the queue.
    ///
    /// Note that this function returns `false` even if all items have not yet
    /// expired and a call to `poll` will return `NotReady`.
    ///
    /// # Examples
    ///
    /// ```
    /// # use tokio_timer::DelayQueue;
    /// use std::time::Duration;
    /// let mut delay_queue = DelayQueue::new();
    /// assert!(delay_queue.is_empty());
    ///
    /// delay_queue.insert("hello", Duration::from_secs(5));
    /// assert!(!delay_queue.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.slab.is_empty()
    }

    /// Polls the queue, returning the index of the next slot in the slab that
    /// should be returned.
    ///
    /// A slot should be returned when the associated deadline has been reached.
    fn poll_idx(&mut self) -> Poll<Option<usize>, Error> {
        use self::wheel::Stack;

        let expired = self.expired.pop(&mut self.slab);

        if expired.is_some() {
            return Ok(expired.into());
        }

        loop {
            if let Some(ref mut delay) = self.delay {
                if !delay.is_elapsed() {
                    try_ready!(delay.poll());
                }

                let now = ::ms(delay.deadline() - self.start, ::Round::Down);

                self.poll = wheel::Poll::new(now);
            }

            self.delay = None;

            if let Some(idx) = self.wheel.poll(&mut self.poll, &mut self.slab) {
                return Ok(Some(idx).into());
            }

            if let Some(deadline) = self.next_deadline() {
                self.delay = Some(self.handle.delay(deadline));
            } else {
                return Ok(None.into());
            }
        }
    }

    fn normalize_deadline(&self, when: Instant) -> u64 {
        let when = if when < self.start {
            0
        } else {
            ::ms(when - self.start, ::Round::Up)
        };

        cmp::max(when, self.wheel.elapsed())
    }
}

impl<T> Stream for DelayQueue<T> {
    type Item = Expired<T>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Error> {
        let item = try_ready!(self.poll_idx()).map(|idx| {
            let data = self.slab.remove(idx);
            debug_assert!(data.next.is_none());
            debug_assert!(data.prev.is_none());

            Expired {
                key: Key::new(idx),
                data: data.inner,
                deadline: self.start + Duration::from_millis(data.when),
            }
        });

        Ok(item.into())
    }
}

impl<T> wheel::Stack for Stack<T> {
    type Owned = usize;
    type Borrowed = usize;
    type Store = Slab<Data<T>>;

    fn is_empty(&self) -> bool {
        self.head.is_none()
    }

    fn push(&mut self, item: Self::Owned, store: &mut Self::Store) {
        // Ensure the entry is not already in a stack.
        debug_assert!(store[item].next.is_none());
        debug_assert!(store[item].prev.is_none());

        // Remove the old head entry
        let old = self.head.take();

        if let Some(idx) = old {
            store[idx].prev = Some(item);
        }

        store[item].next = old;
        self.head = Some(item)
    }

    fn pop(&mut self, store: &mut Self::Store) -> Option<Self::Owned> {
        if let Some(idx) = self.head {
            self.head = store[idx].next;

            if let Some(idx) = self.head {
                store[idx].prev = None;
            }

            store[idx].next = None;
            debug_assert!(store[idx].prev.is_none());

            Some(idx)
        } else {
            None
        }
    }

    fn remove(&mut self, item: &Self::Borrowed, store: &mut Self::Store) {
        assert!(store.contains(*item));

        // Ensure that the entry is in fact contained by the stack
        debug_assert!({
            // This walks the full linked list even if an entry is found.
            let mut next = self.head;
            let mut contains = false;

            while let Some(idx) = next {
                if idx == *item {
                    debug_assert!(!contains);
                    contains = true;
                }

                next = store[idx].next;
            }

            contains
        });

        if let Some(next) = store[*item].next {
            store[next].prev = store[*item].prev;
        }

        if let Some(prev) = store[*item].prev {
            store[prev].next = store[*item].next;
        } else {
            self.head = store[*item].next;
        }

        store[*item].next = None;
        store[*item].prev = None;
    }

    fn when(item: &Self::Borrowed, store: &Self::Store) -> u64 {
        store[*item].when
    }
}

impl<T> Default for Stack<T> {
    fn default() -> Stack<T> {
        Stack {
            head: None,
            _p: PhantomData,
        }
    }
}

impl Key {
    pub(crate) fn new(index: usize) -> Key {
        Key { index }
    }
}

impl<T> Expired<T> {
    /// Returns a reference to the inner value.
    pub fn get_ref(&self) -> &T {
        &self.data
    }

    /// Returns a mutable reference to the inner value.
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// Consumes `self` and returns the inner value.
    pub fn into_inner(self) -> T {
        self.data
    }
}
