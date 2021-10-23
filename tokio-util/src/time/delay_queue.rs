//! A queue of delayed elements.
//!
//! See [`DelayQueue`] for more details.
//!
//! [`DelayQueue`]: struct@DelayQueue

use crate::time::wheel::{self, Wheel};

use futures_core::ready;
use tokio::time::{error::Error, sleep_until, Duration, Instant, Sleep};

use slab::Slab;
use std::cmp;
use std::collections::HashMap;
use std::fmt::Debug;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::task::{self, Poll, Waker};

// Size of list that is used to store keys that can be used by `create_new_key`.
const AVAILABLE_KEYS_LIST_SIZE: usize = 200;

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
/// [`Stream`] implementation. [`poll_expired`] is called. If an entry has reached its
/// deadline, it is returned. If not, `Poll::Pending` is returned indicating that the
/// current task will be notified once the deadline has been reached.
///
/// # `Stream` implementation
///
/// Items are retrieved from the queue via [`DelayQueue::poll_expired`]. If no delays have
/// expired, no items are returned. In this case, `Poll::Pending` is returned and the
/// current task is registered to be notified once the next item's delay has
/// expired.
///
/// If no items are in the queue, i.e. `is_empty()` returns `true`, then `poll`
/// returns `Poll::Ready(None)`. This indicates that the stream has reached an end.
/// However, if a new item is inserted *after*, `poll` will once again start
/// returning items or `Poll::Pending`.
///
/// Items are returned ordered by their expirations. Items that are configured
/// to expire first will be returned first. There are no ordering guarantees
/// for items configured to expire at the same instant. Also note that delays are
/// rounded to the closest millisecond.
///
/// # Implementation
///
/// The [`DelayQueue`] is backed by a separate instance of a timer wheel similar to that used internally
/// by Tokio's standalone timer utilities such as [`sleep`]. Because of this, it offers the same
/// performance and scalability benefits.
///
/// State associated with each entry is stored in a [`slab`]. This amortizes the cost of allocation,
/// and allows reuse of the memory allocated for expired entires.
///
/// Capacity can be checked using [`capacity`] and allocated preemptively by using
/// the [`reserve`] method.
///
/// # Usage
///
/// Using `DelayQueue` to manage cache entries.
///
/// ```rust,no_run
/// use tokio::time::error::Error;
/// use tokio_util::time::{DelayQueue, delay_queue};
///
/// use futures::ready;
/// use std::collections::HashMap;
/// use std::task::{Context, Poll};
/// use std::time::Duration;
/// # type CacheKey = String;
/// # type Value = String;
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
///     fn poll_purge(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Error>> {
///         while let Some(res) = ready!(self.expirations.poll_expired(cx)) {
///             let entry = res?;
///             self.entries.remove(entry.get_ref());
///         }
///
///         Poll::Ready(Ok(()))
///     }
/// }
/// ```
///
/// [`insert`]: method@Self::insert
/// [`insert_at`]: method@Self::insert_at
/// [`Key`]: struct@Key
/// [`Stream`]: https://docs.rs/futures/0.1/futures/stream/trait.Stream.html
/// [`poll_expired`]: method@Self::poll_expired
/// [`Stream::poll_expired`]: method@Self::poll_expired
/// [`DelayQueue`]: struct@DelayQueue
/// [`sleep`]: fn@tokio::time::sleep
/// [`slab`]: slab
/// [`capacity`]: method@Self::capacity
/// [`reserve`]: method@Self::reserve
#[derive(Debug)]
pub struct DelayQueue<T> {
    /// Stores data associated with entries
    slab: Slab<Data<T>>,

    /// Lookup structure tracking all delays in the queue
    wheel: Wheel<Stack<T>>,

    /// Delays that were inserted when already expired. These cannot be stored
    /// in the wheel
    expired: Stack<T>,

    /// Delay expiring when the *first* item in the queue expires
    delay: Option<Pin<Box<Sleep>>>,

    /// Wheel polling state
    wheel_now: u64,

    /// Instant at which the timer starts
    start: Instant,

    /// Waker that is invoked when we potentially need to reset the timer.
    /// Because we lazily create the timer when the first entry is created, we
    /// need to awaken any poller that polled us before that point.
    waker: Option<Waker>,

    /// A `compact` call requires a re-mapping of the `Key`s that were changed
    /// during the `compact` call of the `slab`. Since the keys that were given out
    /// cannot be changed retroactively we need to keep track of these re-mappings.
    /// The keys of `key_map` correspond to the old keys that were given out and
    /// the values to the `Key`s that were re-mapped by the `compact` call.
    key_map: HashMap<Key, Key>,

    /// List of keys that we can use to create new keys. See the comment for
    /// `create_available_keys` for why this is necessary.
    available_keys: Vec<usize>,
}

/// An entry in `DelayQueue` that has expired and been removed.
///
/// Values are returned by [`DelayQueue::poll_expired`].
///
/// [`DelayQueue::poll_expired`]: method@DelayQueue::poll_expired
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
/// [`DelayQueue`]: struct@DelayQueue
/// [`DelayQueue::insert`]: method@DelayQueue::insert
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Key {
    index: usize,
}

#[derive(Debug)]
struct Stack<T> {
    /// Head of the stack
    head: Option<usize>,
    _p: PhantomData<fn() -> T>,
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
    /// Creates a new, empty, `DelayQueue`.
    ///
    /// The queue will not allocate storage until items are inserted into it.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio_util::time::DelayQueue;
    /// let delay_queue: DelayQueue<u32> = DelayQueue::new();
    /// ```
    pub fn new() -> DelayQueue<T> {
        DelayQueue::with_capacity(0)
    }

    /// Creates a new, empty, `DelayQueue` with the specified capacity.
    ///
    /// The queue will be able to hold at least `capacity` elements without
    /// reallocating. If `capacity` is 0, the queue will not allocate for
    /// storage.
    ///
    /// # Examples
    ///
    /// ```rust
    /// # use tokio_util::time::DelayQueue;
    /// # use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue = DelayQueue::with_capacity(10);
    ///
    /// // These insertions are done without further allocation
    /// for i in 0..10 {
    ///     delay_queue.insert(i, Duration::from_secs(i));
    /// }
    ///
    /// // This will make the queue allocate additional storage
    /// delay_queue.insert(11, Duration::from_secs(11));
    /// # }
    /// ```
    pub fn with_capacity(capacity: usize) -> DelayQueue<T> {
        DelayQueue {
            wheel: Wheel::new(),
            slab: Slab::with_capacity(capacity),
            expired: Stack::default(),
            delay: None,
            wheel_now: 0,
            start: Instant::now(),
            waker: None,
            key_map: HashMap::new(),
            available_keys: Vec::new(),
        }
    }

    /// Inserts `value` into the queue set to expire at a specific instant in
    /// time.
    ///
    /// This function is identical to `insert`, but takes an `Instant` instead
    /// of a `Duration`.
    ///
    /// `value` is stored in the queue until `when` is reached. At which point,
    /// `value` will be returned from [`poll_expired`]. If `when` has already been
    /// reached, then `value` is immediately made available to poll.
    ///
    /// The return value represents the insertion and is used as an argument to
    /// [`remove`] and [`reset`]. Note that [`Key`] is a token and is reused once
    /// `value` is removed from the queue either by calling [`poll_expired`] after
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
    /// use tokio::time::{Duration, Instant};
    /// use tokio_util::time::DelayQueue;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
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
    /// [`poll_expired`]: method@Self::poll_expired
    /// [`remove`]: method@Self::remove
    /// [`reset`]: method@Self::reset
    /// [`Key`]: struct@Key
    /// [type]: #
    pub fn insert_at(&mut self, value: T, when: Instant) -> Key {
        assert!(self.slab.len() < MAX_ENTRIES, "max entries exceeded");

        // Normalize the deadline. Values cannot be set to expire in the past.
        let when = self.normalize_deadline(when);

        // Insert the value in the store
        let mut key = self.slab.insert(Data {
            inner: value,
            when,
            expired: false,
            next: None,
            prev: None,
        });
        let old_key = key;

        if self.key_map.contains_key(&Key::new(key)) {
            // It's possible that a `compact` call creates capacitiy in `self.slab` in
            // such a way that a `self.slab.insert` call creates a `key` which was
            // previously given out during an `insert` call prior to the `compact` call.
            // If `key` is contained in `self.key_map`, we have encountered this exact situation,
            // We need to create a new key `key_to_give_out` and include the relation
            // `key_to_give_out` -> `key` in `self.key_map`.
            let key_to_give_out = self.create_new_key();
            self.key_map
                .insert(Key::new(key_to_give_out), Key::new(key));
            key = key_to_give_out
        }

        // `old_key` is the actual index the slab uses internally
        self.insert_idx(when, old_key);

        // Set a new delay if the current's deadline is later than the one of the new item
        let should_set_delay = if let Some(ref delay) = self.delay {
            let current_exp = self.normalize_deadline(delay.deadline());
            current_exp > when
        } else {
            true
        };

        if should_set_delay {
            if let Some(waker) = self.waker.take() {
                waker.wake();
            }

            let delay_time = self.start + Duration::from_millis(when);
            if let Some(ref mut delay) = &mut self.delay {
                delay.as_mut().reset(delay_time);
            } else {
                self.delay = Some(Box::pin(sleep_until(delay_time)));
            }
        }

        Key::new(key)
    }

    // We use a linked list of available keys that we can use to create
    // new keys. The creation of new keys is necessary if the `slab.insert` call
    // in `self.insert` gives back a key that was previously given out.
    // This scenario of a duplicate key can only happen after `compact` was called.
    // We maintain a list of available keys for efficiency reasons, so as not to calculate
    // the smallest available key each time `slab.insert` outputs a duplicate key.
    fn create_available_keys(&mut self) {
        assert!(self.available_keys.is_empty());

        let mut i = 0;
        let mut num_created_keys = 0;
        while num_created_keys < AVAILABLE_KEYS_LIST_SIZE {
            if !self.key_map.contains_key(&Key::new(i)) {
                self.available_keys.push(i);
                num_created_keys += 1;
            }
            i += 1;
        }
    }

    fn create_new_key(&mut self) -> usize {
        if self.available_keys.is_empty() {
            self.create_available_keys();
        }

        self.available_keys.pop().unwrap()
    }

    /// Attempts to pull out the next value of the delay queue, registering the
    /// current task for wakeup if the value is not yet available, and returning
    /// `None` if the queue is exhausted.
    pub fn poll_expired(
        &mut self,
        cx: &mut task::Context<'_>,
    ) -> Poll<Option<Result<Expired<T>, Error>>> {
        if !self
            .waker
            .as_ref()
            .map(|w| w.will_wake(cx.waker()))
            .unwrap_or(false)
        {
            self.waker = Some(cx.waker().clone());
        }

        let item = ready!(self.poll_idx(cx));
        Poll::Ready(item.map(|result| {
            result.map(|idx| {
                let data = self.slab.remove(idx);
                debug_assert!(data.next.is_none());
                debug_assert!(data.prev.is_none());

                Expired {
                    key: Key::new(idx),
                    data: data.inner,
                    deadline: self.start + Duration::from_millis(data.when),
                }
            })
        }))
    }

    /// Inserts `value` into the queue set to expire after the requested duration
    /// elapses.
    ///
    /// This function is identical to `insert_at`, but takes a `Duration`
    /// instead of an `Instant`.
    ///
    /// `value` is stored in the queue until `timeout` duration has
    /// elapsed after `insert` was called. At that point, `value` will
    /// be returned from [`poll_expired`]. If `timeout` is a `Duration` of
    /// zero, then `value` is immediately made available to poll.
    ///
    /// The return value represents the insertion and is used as an
    /// argument to [`remove`] and [`reset`]. Note that [`Key`] is a
    /// token and is reused once `value` is removed from the queue
    /// either by calling [`poll_expired`] after `timeout` has elapsed
    /// or by calling [`remove`]. At this point, the caller must not
    /// use the returned [`Key`] again as it may reference a different
    /// item in the queue.
    ///
    /// See [type] level documentation for more details.
    ///
    /// # Panics
    ///
    /// This function panics if `timeout` is greater than the maximum
    /// duration supported by the timer in the current `Runtime`.
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```rust
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// // Remove the entry
    /// let item = delay_queue.remove(&key);
    /// assert_eq!(*item.get_ref(), "foo");
    /// # }
    /// ```
    ///
    /// [`poll_expired`]: method@Self::poll_expired
    /// [`remove`]: method@Self::remove
    /// [`reset`]: method@Self::reset
    /// [`Key`]: struct@Key
    /// [type]: #
    pub fn insert(&mut self, value: T, timeout: Duration) -> Key {
        self.insert_at(value, Instant::now() + timeout)
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

    /// Removes the key from the expired queue or the timer wheel
    /// depending on its expiration status.
    ///
    /// # Panics
    ///
    /// Panics if the key is not contained in the expired queue or the wheel.
    fn remove_key(&mut self, key: &Key) {
        use crate::time::wheel::Stack;

        // Special case the `expired` queue
        if self.slab[key.index].expired {
            self.expired.remove(&key.index, &mut self.slab);
        } else {
            self.wheel.remove(&key.index, &mut self.slab);
        }
    }

    /// Removes the item associated with `key` from the queue.
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
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// // Remove the entry
    /// let item = delay_queue.remove(&key);
    /// assert_eq!(*item.get_ref(), "foo");
    /// # }
    /// ```
    pub fn remove(&mut self, key: &Key) -> Expired<T> {
        let prev_deadline = self.next_deadline();

        let remapped_key = self.key_map.remove(key).unwrap_or(*key);
        self.remove_key(&remapped_key);
        let data = self.slab.remove(remapped_key.index);

        let next_deadline = self.next_deadline();
        if prev_deadline != next_deadline {
            match (next_deadline, &mut self.delay) {
                (None, _) => self.delay = None,
                (Some(deadline), Some(delay)) => delay.as_mut().reset(deadline),
                (Some(deadline), None) => self.delay = Some(Box::pin(sleep_until(deadline))),
            }
        }

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
    /// use tokio::time::{Duration, Instant};
    /// use tokio_util::time::DelayQueue;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// let key = delay_queue.insert("foo", Duration::from_secs(5));
    ///
    /// // "foo" is scheduled to be returned in 5 seconds
    ///
    /// delay_queue.reset_at(&key, Instant::now() + Duration::from_secs(10));
    ///
    /// // "foo" is now scheduled to be returned in 10 seconds
    /// # }
    /// ```
    pub fn reset_at(&mut self, key: &Key, when: Instant) {
        let key_map = &self.key_map;
        let remapped_key = *key_map.get(&*key).unwrap_or(key);

        self.remove_key(&remapped_key);

        // Normalize the deadline. Values cannot be set to expire in the past.
        let when = self.normalize_deadline(when);

        self.slab[remapped_key.index].when = when;
        self.slab[remapped_key.index].expired = false;

        self.insert_idx(when, remapped_key.index);

        let next_deadline = self.next_deadline();
        if let (Some(ref mut delay), Some(deadline)) = (&mut self.delay, next_deadline) {
            // This should awaken us if necessary (ie, if already expired)
            delay.as_mut().reset(deadline);
        }
    }

    /// Shrink the capacity of the slab, which `DelayQueue` uses internally for storage allocation.
    /// This function is not guaranteed to, and in most cases, won't decrease the capacity of the slab
    /// to the number of elements still contained in it. To decrease the capacity to the size of
    /// the slab use [`compact`]
    /// This function can take O(n) time even when the capacity cannot be reduced or the allocation is
    /// shrunk in place. Repeated calls run in O(1) though.
    ///
    /// [`compact`]: method@Self::compact
    pub fn shrink_to_fit(&mut self) {
        self.slab.shrink_to_fit()
    }

    /// Shrink the capacity of the slab, which `DelayQueue` uses internally for storage allocation,
    /// to the number of elements that are contained in it.
    ///
    /// This methods runs in O(n).
    ///
    /// # Examples
    ///
    /// Basic usage
    ///
    /// ```rust
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue = DelayQueue::with_capacity(10);
    ///
    /// let key1 = delay_queue.insert(5, Duration::from_secs(5));
    /// let key2 = delay_queue.insert(10, Duration::from_secs(10));
    /// let key3 = delay_queue.insert(15, Duration::from_secs(15));
    ///
    /// delay_queue.remove(&key3);
    ///
    /// delay_queue.compact();
    /// assert_eq!(delay_queue.capacity(), 2);
    /// # }
    /// ```
    pub fn compact(&mut self) {
        // We need to invert `key_map` since during a `slab.compact` call
        // previously re-mapped keys (the values in `key_map`) might be re-mapped.
        let key_map = &mut self.key_map;
        let slab = &mut self.slab;
        let wheel = &mut self.wheel;

        let mut remapped_keys = HashMap::new();

        let mut inverse_key_map: HashMap<Key, Key> = HashMap::new();
        for (old_key, remapped_key) in key_map.iter() {
            inverse_key_map.insert(*remapped_key, *old_key);
        }

        slab.compact(|_, from, to| {
            remapped_keys.insert(from, to);

            let from_key = Key::new(from);
            let to_key = Key::new(to);

            if inverse_key_map.contains_key(&from_key) {
                // `from_key` is a `Key` that was previously re-mapped, get the key
                // that was previously given out for its item.
                let old_key = inverse_key_map.get(&from_key).unwrap();

                *key_map.get_mut(&old_key).unwrap() = to_key;
            } else {
                let key = key_map.get_mut(&from_key);
                match key {
                    Some(k) => *k = to_key,
                    None => {
                        key_map.insert(from_key, to_key);
                    }
                }
            }

            true
        });

        // The wheel internally uses the `Key` indices in the slots of its levels,
        // these `Key`s also need to be re-mapped.
        wheel.adjust_indices(&remapped_keys, slab);
    }

    /// Returns the next time to poll as determined by the wheel
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
    /// `timeout`. If `timeout` is zero, then the item is immediately made
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
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
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
        self.reset_at(key, Instant::now() + timeout);
    }

    /// Clears the queue, removing all items.
    ///
    /// After calling `clear`, [`poll_expired`] will return `Ok(Ready(None))`.
    ///
    /// Note that this method has no effect on the allocated capacity.
    ///
    /// [`poll_expired`]: method@Self::poll_expired
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
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
    /// use tokio_util::time::DelayQueue;
    ///
    /// let delay_queue: DelayQueue<i32> = DelayQueue::with_capacity(10);
    /// assert_eq!(delay_queue.capacity(), 10);
    /// ```
    pub fn capacity(&self) -> usize {
        self.slab.capacity()
    }

    /// Returns the number of elements currently in the queue.
    ///
    /// # Examples
    ///
    /// ```rust
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue: DelayQueue<i32> = DelayQueue::with_capacity(10);
    /// assert_eq!(delay_queue.len(), 0);
    /// delay_queue.insert(3, Duration::from_secs(5));
    /// assert_eq!(delay_queue.len(), 1);
    /// # }
    /// ```
    pub fn len(&self) -> usize {
        self.slab.len()
    }

    /// Reserves capacity for at least `additional` more items to be queued
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
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue = DelayQueue::new();
    ///
    /// delay_queue.insert("hello", Duration::from_secs(10));
    /// delay_queue.reserve(10);
    ///
    /// assert!(delay_queue.capacity() >= 11);
    /// # }
    /// ```
    pub fn reserve(&mut self, additional: usize) {
        self.slab.reserve(additional);
    }

    /// Returns `true` if there are no items in the queue.
    ///
    /// Note that this function returns `false` even if all items have not yet
    /// expired and a call to `poll` will return `Poll::Pending`.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio_util::time::DelayQueue;
    /// use std::time::Duration;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut delay_queue = DelayQueue::new();
    /// assert!(delay_queue.is_empty());
    ///
    /// delay_queue.insert("hello", Duration::from_secs(5));
    /// assert!(!delay_queue.is_empty());
    /// # }
    /// ```
    pub fn is_empty(&self) -> bool {
        self.slab.is_empty()
    }

    /// Polls the queue, returning the index of the next slot in the slab that
    /// should be returned.
    ///
    /// A slot should be returned when the associated deadline has been reached.
    fn poll_idx(&mut self, cx: &mut task::Context<'_>) -> Poll<Option<Result<usize, Error>>> {
        use self::wheel::Stack;

        let expired = self.expired.pop(&mut self.slab);

        if expired.is_some() {
            return Poll::Ready(expired.map(Ok));
        }

        loop {
            if let Some(ref mut delay) = self.delay {
                if !delay.is_elapsed() {
                    ready!(Pin::new(&mut *delay).poll(cx));
                }

                let now = crate::time::ms(delay.deadline() - self.start, crate::time::Round::Down);

                self.wheel_now = now;
            }

            // We poll the wheel to get the next value out before finding the next deadline.
            let wheel_idx = self.wheel.poll(self.wheel_now, &mut self.slab);

            self.delay = self.next_deadline().map(|when| Box::pin(sleep_until(when)));

            if let Some(idx) = wheel_idx {
                return Poll::Ready(Some(Ok(idx)));
            }

            if self.delay.is_none() {
                return Poll::Ready(None);
            }
        }
    }

    fn normalize_deadline(&self, when: Instant) -> u64 {
        let when = if when < self.start {
            0
        } else {
            crate::time::ms(when - self.start, crate::time::Round::Up)
        };

        cmp::max(when, self.wheel.elapsed())
    }
}

// We never put `T` in a `Pin`...
impl<T> Unpin for DelayQueue<T> {}

impl<T> Default for DelayQueue<T> {
    fn default() -> DelayQueue<T> {
        DelayQueue::new()
    }
}

impl<T> futures_core::Stream for DelayQueue<T> {
    // DelayQueue seems much more specific, where a user may care that it
    // has reached capacity, so return those errors instead of panicking.
    type Item = Result<Expired<T>, Error>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut task::Context<'_>) -> Poll<Option<Self::Item>> {
        DelayQueue::poll_expired(self.get_mut(), cx)
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
        self.head = Some(item);
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

    fn modify_items<F>(&mut self, store: &mut Self::Store, f: F)
    where
        F: Fn(Self::Borrowed) -> Self::Borrowed,
    {
        if !self.is_empty() {
            self.head = self.head.map(|h| f(h));

            let mut current_index = self.head.unwrap();
            let mut prev_index;
            let mut current_elem = &mut store[current_index];

            while let Some(next_index) = current_elem.next {
                prev_index = current_index;
                current_index = f(next_index);
                current_elem.next = Some(current_index);
                current_elem = &mut store[current_index];
                current_elem.prev = Some(prev_index);
            }
        }
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

    /// Returns the deadline that the expiration was set to.
    pub fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Returns the key that the expiration is indexed by.
    pub fn key(&self) -> Key {
        self.key
    }
}
