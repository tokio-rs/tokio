use crate::loom::sync::Arc;
use crate::runtime::Handle;
use crate::task::{JoinError, JoinHandle, LocalSet};
use crate::util::idle_notified_set::{self, IdleNotifiedSet};
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use std::collections::HashSet;
use std::fmt;
use std::future::Future;
use std::hash::{BuildHasher, Hash, Hasher};
use std::pin::Pin;
use std::task::{Context, Poll};

/// A collection of tasks spawned on a Tokio runtime, associated with hash map
/// keys.
///
/// This type is very similar to the [`JoinSet`] type, with the addition of a
/// set of keys associated with each task. These keys allow [cancelling a
/// task][abort] or [multiple tasks][abort_matching] in the `JoinMap` based on
/// their keys, or [test whether a task corresponding to a given key exists][contains] in the `JoinMap`.
///
/// In addition, when tasks in the `JoinMap` complete, they will return the
/// associated key along with the value returned by the task, if any.
///
/// A `JoinMap` can be used to await the completion of some or all of the tasks
/// in the map. The map is not ordered, and the tasks will be returned in the
/// order they complete.
///
/// All of the tasks must have the same return type `V`.
///
/// When the `JoinMap` is dropped, all tasks in the `JoinMap` are immediately aborted.
///
/// **Note**: This is an [unstable API][unstable]. The public API of this type
/// may break in 1.x releases. See [the documentation on unstable
/// features][unstable] for details.
///
/// # Examples
///
/// Spawn multiple tasks and wait for them:
///
/// ```
/// use tokio::task::JoinMap;
///
/// #[tokio::main]
/// async fn main() {
///     let mut map = JoinMap::new();
///
///     for i in 0..10 {
///         // Spawn a task on the `JoinMap` with `i` as its key.
///         map.spawn(i, async move { /* ... */ });
///     }
///
///     let mut seen = [false; 10];
///
///     // When a task completes, `join_one` returns the task's key along
///     // with its output.
///     while let Some((key, res)) = map.join_one().await {
///         seen[key] = true;
///         assert!(res.is_ok(), "task {} completed successfully!", key);
///     }
///
///     for i in 0..10 {
///         assert!(seen[i]);
///     }
/// }
/// ```
///
/// Cancel tasks based on their keys:
///
/// ```
/// use tokio::task::JoinMap;
///
/// #[tokio::main]
/// async fn main() {
///     let mut map = JoinMap::new();
///
///     map.spawn("hello world", async move { /* ... */ });
///     map.spawn("goodbye world", async move { /* ... */});
///
///     // Look up the "goodbye world" task in the map and abort it.
///     let aborted = map.abort("goodbye world");
///
///     // `JoinMap::abort` returns `true` if a task existed for the
///     // provided key.
///     assert!(aborted);
///
///     while let Some((key, res)) = map.join_one().await {
///         if key == "goodbye world" {
///             // The aborted task should complete with a cancelled `JoinError`.
///             assert!(res.unwrap_err().is_cancelled());
///         } else {
///             // Other tasks should complete normally.
///             assert!(res.is_ok());
///         }
///     }
/// }
/// ```
///
/// [`JoinSet`]: crate::task::JoinSet
/// [unstable]: crate#unstable-features
/// [abort]: fn@Self::abort
/// [abort_matching]: fn@Self::abort_matching
/// [contains]: fn@Self::contains_task
#[cfg_attr(docsrs, doc(cfg(all(feature = "rt", tokio_unstable))))]
pub struct JoinMap<K, V, S = RandomState> {
    /// The map's key set.
    ///
    /// Tasks spawned on a `JoinMap` are uniquely identified by a key. To avoid
    /// having to clone the keys, which may be expensive (e.g. if they are
    /// strings...), we store the keys only in the `JoinMap` and *not* in the
    /// tasks themselves. Instead, the key's hash is stored in the
    /// `IdleNotifiedSet`; when a task completes, we look up the key using that
    /// hash, resolving collisions based on whether or not two entries in the
    /// map have the same `Arc` pointer.
    ///
    /// Because we look up entries in this set with a previously computed hash
    /// when a task completes, the `HashSet` uses a no-op `Hasher` that just
    /// returns the value that was passed to `Hasher::write_u64`. Therefore,
    /// when looking up keys by key (rather than by hash), we use a
    /// `PreHashedKey` type that stores a hash computed outside of the
    /// `HashSet`.
    key_set: HashSet<Key<K, V>, NoHash>,

    /// The set of tasks spawned on the `JoinMap`.
    ///
    /// Each `IdleNotifiedSet` entry contains the hash of the task's key, to
    /// allow looking the key up when the task completes.
    task_set: IdleNotifiedSet<(u64, JoinHandle<V>)>,

    hash_builder: S,
}

/// A `JoinMap` key.
///
/// This contains both the actual key value *and* an `Arc` clone of the entry in
/// the `IdleNotifiedSet` for the corresponding task. This way, when looking up
/// completed tasks by their hash, we can hash resolve collisions by testing if the
/// `Arc` points to the same `IdleNotifiedSet` entry.
///
/// Also, the `IdleNotifiedSet` join handle is used to cancel the task in
/// `abort` and `abort_matching`.
struct Key<K, V> {
    key: K,
    task: TaskKey<V>,
}

/// The part of the `JoinMap` key that can be looked up by-task.
///
/// This contains the hash value of the actual key and the `IdleNotifiedSet`
/// entry for the corresponding task. When a task completes, we look it up in
/// the key set using a `TaskKey`, which resolves hash collisions based on
/// whether the `IdleNotifiedSet` entry `Arc`s have the same address, rather
/// than based on key equality.
///
/// This allows storing only the actual key's `u64` hash value in the task's
/// `IdleNotifiedSet` entry, rather than having to clone the entire key into the
/// task.
struct TaskKey<V> {
    hash: u64,
    entry: Arc<idle_notified_set::ListEntry<(u64, JoinHandle<V>)>>,
}

/// A `Q`-typed map key with a pre-computed hash.
///
/// This is used when looking up key set entries with a `Q`-typed key (such as
/// in `contains_task` and `abort`). Because the key set `HashSet` uses a no-op
/// hasher to permit looking up stored hash values on task completion,
struct PreHashedKey<'a, Q: ?Sized> {
    hash: u64,
    key: &'a Q,
}

/// This trait is used as a workaround for the fact that we can't implement
/// `Borrow<Q> for Key<K, V> where K: Borrow<Q>`, which is unfortunately not
/// possible due to the blanket impl of `Borrow<T> for T`.
///
/// Not being able to implement `Borrow<Q>` for the `Key` type where `Q` is a
/// borrowed form of `K` would mean that we could only call `HashSet::get` with
/// `K`-typed keys (and not borrowed keys). This trait solves that problem by
/// allowing us to implement `DynKey<Q> for Key<K, V> where K: Borrow<Q>`, and
/// then have a `Borrow<dyn DynKey<Q>>` impl for `Key`. That way, the `Key<K,
/// V>` type can be borrowed as any borrowed form of `K` while still being a
/// more complex type than _just_ a `K`.
trait DynKey<Q: ?Sized> {
    fn key(&self) -> &Q;
    fn hash_value(&self) -> u64;
}

/// The world's fastest hashing algorithm.
#[derive(Default)]
struct NoHash(u64);

impl<K, V> JoinMap<K, V> {
    /// Creates a new empty `JoinMap`.
    ///
    /// The `JoinMap` is initially created with a capacity of 0, so it will not
    /// allocate until a task is first spawned on it.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::task::JoinMap;
    /// let map: JoinMap<&str, i32> = JoinMap::new();
    /// ```
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }

    /// Creates an empty `JoinMap` with the specified capacity.
    ///
    /// The `JoinMap` will be able to hold at least `capacity` tasks without
    /// reallocating.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::task::JoinMap;
    /// let map: JoinMap<&str, i32> = JoinMap::with_capacity(10);
    /// ```
    #[inline]
    #[must_use]
    pub fn with_capacity(capacity: usize) -> Self {
        JoinMap::with_capacity_and_hasher(capacity, Default::default())
    }
}

impl<K, V, S> JoinMap<K, V, S> {
    /// Creates an empty `JoinMap` which will use the given hash builder to hash
    /// keys.
    ///
    /// The created map has the default initial capacity.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and
    /// is designed to allow `JoinMap` to be resistant to attacks that
    /// cause many collisions and very poor performance. Setting it
    /// manually using this function can expose a DoS attack vector.
    ///
    /// The `hash_builder` passed should implement the [`BuildHasher`] trait for
    /// the `JoinMap` to be useful, see its documentation for details.
    #[inline]
    #[must_use]
    pub fn with_hasher(hash_builder: S) -> Self {
        Self::with_capacity_and_hasher(0, hash_builder)
    }

    /// Creates an empty `JoinMap` with the specified capacity, using `hash_builder`
    /// to hash the keys.
    ///
    /// The `JoinMap` will be able to hold at least `capacity` elements without
    /// reallocating. If `capacity` is 0, the `JoinMap` will not allocate.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and
    /// is designed to allow HashMaps to be resistant to attacks that
    /// cause many collisions and very poor performance. Setting it
    /// manually using this function can expose a DoS attack vector.
    ///
    /// The `hash_builder` passed should implement the [`BuildHasher`] trait for
    /// the `JoinMap`to be useful, see its documentation for details.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::task::JoinMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let mut map = JoinMap::with_capacity_and_hasher(10, s);
    /// map.spawn(1, async move { "hello world!" });
    /// # }
    /// ```
    #[inline]
    #[must_use]
    pub fn with_capacity_and_hasher(capacity: usize, hash_builder: S) -> Self {
        Self {
            key_set: HashSet::with_capacity_and_hasher(capacity, NoHash(0)),
            task_set: IdleNotifiedSet::new(),
            hash_builder,
        }
    }

    /// Returns the number of tasks currently in the `JoinMap`.
    pub fn len(&self) -> usize {
        self.task_set.len()
    }

    /// Returns whether the `JoinMap` is empty.
    pub fn is_empty(&self) -> bool {
        self.task_set.is_empty()
    }

    /// Returns the number of tasks the map can hold without reallocating.
    ///
    /// This number is a lower bound; the `JoinMap` might be able to hold
    /// more, but is guaranteed to be able to hold at least this many.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::task::JoinMap;
    ///
    /// let map: JoinMap<i32, i32> = JoinMap::with_capacity(100);
    /// assert!(map.capacity() >= 100);
    /// ```
    #[inline]
    pub fn capacity(&self) -> usize {
        self.key_set.capacity()
    }
}

impl<K, V, S> JoinMap<K, V, S>
where
    K: Hash + Eq + 'static,
    V: 'static,
    S: BuildHasher,
{
    /// Spawn the provided task and store it in this `JoinMap` with the provided
    /// key.
    ///
    /// If a task previously existed in the `JoinMap` for this key, that task
    /// will be cancelled and replaced with the new one. The previous task will
    /// be removed from the `JoinMap`; a subsequent call to [`join_one`] will
    /// *not* return a cancelled [`JoinError`] for that task.
    ///
    /// # Panics
    ///
    /// This method panics if called outside of a Tokio runtime.
    ///
    /// [`join_one`]: Self::join_one
    pub fn spawn<F>(&mut self, key: K, task: F)
    where
        F: Future<Output = V>,
        F: Send + 'static,
        V: Send,
    {
        self.insert(key, crate::spawn(task))
    }

    /// Spawn the provided task on the provided runtime and store it in this
    /// `JoinMap` with the provided key.
    ///
    /// If a task previously existed in the `JoinMap` for this key, that task
    /// will be cancelled and replaced with the new one. The previous task will
    /// be removed from the `JoinMap`; a subsequent call to [`join_one`] will
    /// *not* return a cancelled [`JoinError`] for that task.
    ///
    /// [`join_one`]: Self::join_one
    pub fn spawn_on<F>(&mut self, key: K, task: F, handle: &Handle)
    where
        F: Future<Output = V>,
        F: Send + 'static,
        V: Send,
    {
        self.insert(key, handle.spawn(task));
    }

    /// Spawn the provided task on the current [`LocalSet`] and store it in this
    /// `JoinMap` with the provided key.
    ///
    /// If a task previously existed in the `JoinMap` for this key, that task
    /// will be cancelled and replaced with the new one. The previous task will
    /// be removed from the `JoinMap`; a subsequent call to [`join_one`] will
    /// *not* return a cancelled [`JoinError`] for that task.
    ///
    /// # Panics
    ///
    /// This method panics if it is called outside of a `LocalSet`.
    ///
    /// [`LocalSet`]: crate::task::LocalSet
    /// [`join_one`]: Self::join_one
    pub fn spawn_local<F>(&mut self, key: K, task: F)
    where
        F: Future<Output = V>,
        F: 'static,
    {
        self.insert(key, crate::task::spawn_local(task));
    }

    /// Spawn the provided task on the provided [`LocalSet`] and store it in
    /// this `JoinMap` with the provided key.
    ///
    /// If a task previously existed in the `JoinMap` for this key, that task
    /// will be cancelled and replaced with the new one. The previous task will
    /// be removed from the `JoinMap`; a subsequent call to [`join_one`] will
    /// *not* return a cancelled [`JoinError`] for that task.
    ///
    /// [`LocalSet`]: crate::task::LocalSet
    /// [`join_one`]: Self::join_one
    pub fn spawn_local_on<F>(&mut self, key: K, task: F, local_set: &LocalSet)
    where
        F: Future<Output = V>,
        F: 'static,
    {
        self.insert(key, local_set.spawn_local(task))
    }

    fn insert(&mut self, key: K, jh: JoinHandle<V>) {
        let hash = Self::hash(&self.hash_builder, &key);

        let mut entry = self.task_set.insert_idle((hash, jh));

        // Set the waker that is notified when the task completes.
        entry.with_value_and_context(|(_, jh), ctx| jh.set_join_waker(ctx.waker()));

        let key = Key {
            task: TaskKey {
                hash,
                entry: entry.into_entry(),
            },
            key,
        };

        // Insert the new key into the key set.
        let prev = self
            .key_set
            .replace(key)
            .and_then(|Key { task, .. }| self.task_set.entry_mut(task.entry));

        // If we replaced a previous task in the key set, remove that task from
        // the value set and abort it.
        if let Some(prev) = prev {
            let (_hash, jh) = prev.remove();
            debug_assert_eq!(_hash, hash);
            jh.abort();
        }
    }

    /// Waits until one of the tasks in the map completes and returns its
    /// output, along with the key corresponding to that task.
    ///
    /// Returns `None` if the map is empty.
    ///
    /// # Cancel Safety
    ///
    /// This method is cancel safe. If `join_one` is used as the event in a `tokio::select!`
    /// statement and some other branch completes first, it is guaranteed that no tasks were
    /// removed from this `JoinMap`.
    ///
    /// # Returns
    ///
    /// This function returns:
    ///
    ///  * `Some((key, Ok(value)))` if one of the tasks in this `JoinMap` has
    ///    completed. The `value` is the return value of that ask, and `key` is
    ///    the key associated with the task.
    ///  * `Some((key, Err(err))` if one of the tasks in this JoinMap` has
    ///    panicked or been aborted. `key` is the key associated  with the task
    ///    that panicked or was aborted.
    ///  * `None` if the `JoinMap` is empty.
    pub async fn join_one(&mut self) -> Option<(K, Result<V, JoinError>)> {
        crate::future::poll_fn(|cx| self.poll_join_one(cx)).await
    }

    /// Aborts all tasks and waits for them to finish shutting down.
    ///
    /// Calling this method is equivalent to calling [`abort_all`] and then calling [`join_one`] in
    /// a loop until it returns `None`.
    ///
    /// This method ignores any panics in the tasks shutting down. When this call returns, the
    /// `JoinMap` will be empty.
    ///
    /// [`abort_all`]: fn@Self::abort_all
    /// [`join_one`]: fn@Self::join_one
    pub async fn shutdown(&mut self) {
        self.abort_all();
        while self.join_one().await.is_some() {}
    }

    /// Abort the task corresponding to the provided `key`.
    ///
    /// If this `JoinMap` contains a task corresponding to `key`, this method
    /// will abort that task and return `true`. Otherwise, if no task exists for
    /// `key`, this method returns `false`.
    ///
    /// # Examples
    ///
    /// Aborting a task by key:
    ///
    /// ```
    /// use tokio::task::JoinMap;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut map = JoinMap::new();
    ///
    /// map.spawn("hello world", async move { /* ... */ });
    /// map.spawn("goodbye world", async move { /* ... */});
    ///
    /// // Look up the "goodbye world" task in the map and abort it.
    /// map.abort("goodbye world");
    ///
    /// while let Some((key, res)) = map.join_one().await {
    ///     if key == "goodbye world" {
    ///         // The aborted task should complete with a cancelled `JoinError`.
    ///         assert!(res.unwrap_err().is_cancelled());
    ///     } else {
    ///         // Other tasks should complete normally.
    ///         assert!(res.is_ok());
    ///     }
    /// }
    /// # }
    /// ```
    ///
    /// `abort` returns `true` if a task was aborted:
    /// ```
    /// use tokio::task::JoinMap;
    ///
    /// # #[tokio::main]
    /// # async fn main() {
    /// let mut map = JoinMap::new();
    ///
    /// map.spawn("hello world", async move { /* ... */ });
    /// map.spawn("goodbye world", async move { /* ... */});
    ///
    /// // A task for the key "goodbye world" should exist in the map:
    /// assert!(map.abort("goodbye world"));
    ///
    /// // Aborting a key that does not exist will return `false`:
    /// assert!(!map.abort("goodbye universe"));
    /// # }
    /// ```
    /// ```
    pub fn abort<Q: ?Sized>(&mut self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        let key = self.pre_hashed_key(key);
        let task_set = &mut self.task_set;
        let entry = match self.key_set.get(&key as &dyn DynKey<Q>) {
            Some(entry) => entry,
            None => return false,
        };

        if let Some(mut entry) = task_set.entry_mut(entry.task.entry.clone()) {
            entry.with_value_and_context(|(_actual_hash, jh), _| {
                jh.abort();
            });
            return true;
        }

        false
    }

    /// Aborts all tasks with keys matching `predicate`.
    ///
    /// `predicate` is a function called with a reference to each key in the
    /// map. If it returns `true` for a given key, the corresponding task will
    /// be cancelled.
    ///
    /// # Examples
    /// ```
    /// use tokio::task::JoinMap;
    ///
    /// # // use the current thread rt so that spawned tasks don't
    /// # // complete in the background before they can be aborted.
    /// # #[tokio::main(flavor = "current_thread")]
    /// # async fn main() {
    /// let mut map = JoinMap::new();
    ///
    /// map.spawn("hello world", async move {
    ///     // ...
    ///     # tokio::task::yield_now().await; // don't complete immediately, get aborted!
    /// });
    /// map.spawn("goodbye world", async move {
    ///     // ...
    ///     # tokio::task::yield_now().await; // don't complete immediately, get aborted!
    /// });
    /// map.spawn("hello san francisco", async move {
    ///     // ...
    ///     # tokio::task::yield_now().await; // don't complete immediately, get aborted!
    /// });
    /// map.spawn("goodbye universe", async move {
    ///     // ...
    ///     # tokio::task::yield_now().await; // don't complete immediately, get aborted!
    /// });
    ///
    /// // Abort all tasks whose keys begin with "goodbye"
    /// map.abort_matching(|key| key.starts_with("goodbye"));
    ///
    /// let mut seen = 0;
    /// while let Some((key, res)) = map.join_one().await {
    ///     seen += 1;
    ///     if key.starts_with("goodbye") {
    ///         // The aborted task should complete with a cancelled `JoinError`.
    ///         assert!(res.unwrap_err().is_cancelled());
    ///     } else {
    ///         // Other tasks should complete normally.
    ///         assert!(key.starts_with("hello"));
    ///         assert!(res.is_ok());
    ///     }
    /// }
    ///
    /// // All spawned tasks should have completed.
    /// assert_eq!(seen, 4);
    /// # }
    /// ```
    pub fn abort_matching(&mut self, mut predicate: impl FnMut(&K) -> bool) {
        let key_set = &mut self.key_set;
        let joins = &mut self.task_set;
        // Note: this method iterates over the key set *without* removing any
        // entries, so that the keys from aborted tasks can be returned when
        // polling the `JoinMap`.
        for entry in key_set.iter().filter(|entry| predicate(&entry.key)) {
            if let Some(mut entry) = joins.entry_mut(entry.task.entry.clone()) {
                entry.with_value_and_context(|(_, jh), _| jh.abort());
            }
        }
    }

    /// Returns `true` if this `JoinMap` contains a task for the provided key.
    ///
    /// If the task has completed, but its output hasn't yet been consumed by a
    /// call to [`join_one`], this method will still return `true`.
    ///
    /// [`join_one`]: fn@Self::join_one
    pub fn contains_task<Q: ?Sized>(&mut self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        let key = self.pre_hashed_key(key);
        self.key_set.contains(&key as &dyn DynKey<Q>)
    }

    /// Reserves capacity for at least `additional` more tasks to be spawned
    /// on this `JoinMap` without reallocating for the map of task keys. The
    /// collection may reserve more space to avoid frequent reallocations.
    ///
    /// Note that spawning a task will still cause an allocation for the task
    /// itself.
    ///
    /// # Panics
    ///
    /// Panics if the new allocation size overflows [`usize`].
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::task::JoinMap;
    ///
    /// let mut map: JoinMap<&str, i32> = JoinMap::new();
    /// map.reserve(10);
    /// ```
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.key_set.reserve(additional)
    }

    /// Shrinks the capacity of the `JoinMap` as much as possible. It will drop
    /// down as much as possible while maintaining the internal rules
    /// and possibly leaving some space in accordance with the resize policy.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::task::JoinMap;
    ///
    /// let mut map: JoinMap<i32, i32> = JoinMap::with_capacity(100);
    /// map.spawn(1, async move { 2 });
    /// map.spawn(3, async move { 4 });
    /// assert!(map.capacity() >= 100);
    /// map.shrink_to_fit();
    /// assert!(map.capacity() >= 2);
    /// # }
    /// ```
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.key_set.shrink_to_fit();
    }

    /// Shrinks the capacity of the map with a lower limit. It will drop
    /// down no lower than the supplied limit while maintaining the internal rules
    /// and possibly leaving some space in accordance with the resize policy.
    ///
    /// If the current capacity is less than the lower limit, this is a no-op.
    ///
    /// # Examples
    ///
    /// ```
    /// # #[tokio::main]
    /// # async fn main() {
    /// use tokio::task::JoinMap;
    ///
    /// let mut map: JoinMap<i32, i32> = JoinMap::with_capacity(100);
    /// map.spawn(1, async move { 2 });
    /// map.spawn(3, async move { 4 });
    /// assert!(map.capacity() >= 100);
    /// map.shrink_to(10);
    /// assert!(map.capacity() >= 10);
    /// map.shrink_to(0);
    /// assert!(map.capacity() >= 2);
    /// # }
    /// ```
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.key_set.shrink_to(min_capacity)
    }

    /// Polls for one of the tasks in the map to complete, returning the output
    /// and key of the completed task if one completed.
    ///
    /// If this returns `Poll::Ready(Some((key, _)))`  then the task with the
    /// key `key` completed, and has been removed from the map.
    ///
    /// When the method returns `Poll::Pending`, the `Waker` in the provided `Context` is scheduled
    /// to receive a wakeup when a task in the `JoinSet` completes. Note that on multiple calls to
    /// `poll_join_one`, only the `Waker` from the `Context` passed to the most recent call is
    /// scheduled to receive a wakeup.
    ///
    /// # Returns
    ///
    /// This function returns:
    ///
    ///  * `Poll::Pending` if the `JoinMap` is not empty but there is no task
    ///    whose output is available right now.
    ///  * `Poll::Ready(Some((key, Ok(value))))` if one of the tasks in this
    ///    `JoinMap` has completed. The `value` is the return value of that
    ///    task, and `key` is the key associated with the task.
    ///  * `Poll::Ready(Some((key, Err(err)))` if one of the tasks in this
    ///    `JoinMap` has panicked or has been aborted. `key` is the key
    ///    associated with the task that panicked or was aborted.
    ///  * `Poll::Ready(None)` if the `JoinMap` is empty.
    ///
    /// Note that this method may return `Poll::Pending` even if one of the tasks has completed.
    /// This can happen if the [coop budget] is reached.
    ///
    /// [coop budget]: crate::task#cooperative-scheduling
    fn poll_join_one(&mut self, cx: &mut Context<'_>) -> Poll<Option<(K, Result<V, JoinError>)>> {
        // The call to `pop_notified` moves the entry to the `idle` list. It is moved back to
        // the `notified` list if the waker is notified in the `poll` call below.
        let mut entry = match self.task_set.pop_notified(cx.waker()) {
            Some(entry) => entry,
            None => {
                if self.is_empty() {
                    return Poll::Ready(None);
                } else {
                    // The waker was set by `pop_notified`.
                    return Poll::Pending;
                }
            }
        };

        let res = entry.with_value_and_context(|(_, jh), ctx| Pin::new(jh).poll(ctx));

        if let Poll::Ready(res) = res {
            // Look up the entry in the key map by the hash stored in the
            // `IdleNotifiedSet` entry + the `Arc` pointer.
            let hash = entry.with_value_and_context(|(hash, _), _| *hash);
            let key = TaskKey {
                entry: entry.entry().clone(),
                hash,
            };
            // Remove the entry from the `IdleNotifiedSet`.
            let _ = entry.remove();
            let Key { key, .. } = self.key_set.take(&key).expect(
                "if a task was removed from the key set, it must also have \
                been removed from the IdleNotifiedSet. this is a bug!",
            );

            Poll::Ready(Some((key, res)))
        } else {
            // A JoinHandle generally won't emit a wakeup without being ready unless
            // the coop limit has been reached. We yield to the executor in this
            // case.
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }

    fn pre_hashed_key<'a, Q: ?Sized>(&self, key: &'a Q) -> PreHashedKey<'a, Q>
    where
        Q: Hash + Eq,
    {
        let hash = Self::hash(&self.hash_builder, key);
        PreHashedKey { hash, key }
    }

    fn hash<Q: ?Sized>(hash_builder: &S, key: &Q) -> u64
    where
        Q: Hash,
    {
        let mut hasher = hash_builder.build_hasher();
        key.hash(&mut hasher);
        hasher.finish()
    }
}

impl<K, V, S> JoinMap<K, V, S>
where
    K: 'static,
    V: 'static,
{
    /// Aborts all tasks on this `JoinMap`.
    ///
    /// This does not remove the tasks from the `JoinMap`. To wait for the tasks to complete
    /// cancellation, you should call `join_one` in a loop until the `JoinMap` is empty.
    pub fn abort_all(&mut self) {
        self.task_set.for_each(|(_, jh)| jh.abort());
    }

    /// Removes all tasks from this `JoinMap` without aborting them.
    ///
    /// The tasks removed by this call will continue to run in the background even if the `JoinMap`
    /// is dropped. They may still be aborted by key.
    pub fn detach_all(&mut self) {
        self.task_set.drain(drop);
    }
}

impl<K, V, S> Drop for JoinMap<K, V, S> {
    fn drop(&mut self) {
        self.task_set.drain(|(_, join_handle)| join_handle.abort());
    }
}

impl<K: fmt::Debug + 'static, V: 'static, S> fmt::Debug for JoinMap<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // debug print the keys in this `JoinMap`.
        struct KeySet<'a, K, V, S>(&'a JoinMap<K, V, S>);
        impl<K: fmt::Debug + 'static, V: 'static, S> fmt::Debug for KeySet<'_, K, V, S> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_set()
                    .entries(self.0.key_set.iter().map(|entry| &entry.key))
                    .finish()
            }
        }

        f.debug_struct("JoinMap")
            .field("key_set", &KeySet(self))
            .finish()
    }
}

impl<K, V> Default for JoinMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

// === impl Key ===

impl<K: Hash, V> Hash for Key<K, V> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.task.hash);
    }
}

impl<K: PartialEq, V> PartialEq for Key<K, V> {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key
    }
}

impl<Q: ?Sized, K, V> DynKey<Q> for Key<K, V>
where
    K: Borrow<Q>,
{
    fn key(&self) -> &Q {
        self.key.borrow()
    }

    fn hash_value(&self) -> u64 {
        self.task.hash
    }
}

impl<'a, Q: ?Sized, K, V> Borrow<(dyn DynKey<Q> + 'a)> for Key<K, V>
where
    K: Borrow<Q>,
    K: 'static,
    V: 'static,
{
    fn borrow(&self) -> &(dyn DynKey<Q> + 'a) {
        self
    }
}

impl<K, V> Borrow<TaskKey<V>> for Key<K, V> {
    fn borrow(&self) -> &TaskKey<V> {
        &self.task
    }
}

impl<K: Eq, V> Eq for Key<K, V> {}

// === impl TaskKey ===

impl<V> Hash for TaskKey<V> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.hash);
    }
}

impl<V> PartialEq for TaskKey<V> {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.entry, &other.entry)
    }
}

impl<V> Eq for TaskKey<V> {}

// === impl PreHashedKey ==

impl<Q: ?Sized> DynKey<Q> for PreHashedKey<'_, Q> {
    fn key(&self) -> &Q {
        self.key
    }

    fn hash_value(&self) -> u64 {
        self.hash
    }
}

// === impl (dyn DynKey + 'a) ===

impl<'a, Q: PartialEq + ?Sized> PartialEq for (dyn DynKey<Q> + 'a) {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl<'a, Q: Eq + ?Sized> Eq for (dyn DynKey<Q> + 'a) {}

impl<'a, Q: ?Sized> Hash for (dyn DynKey<Q> + 'a) {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        hasher.write_u64(self.hash_value());
    }
}

// === impl NoHash ===

impl Hasher for NoHash {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!("only `NoHash::write_u64` should be called");
    }

    #[inline]
    fn write_u64(&mut self, hash: u64) {
        debug_assert_eq!(
            self.0, 0,
            "`NoHash::write_u64` should only be called a single time"
        );
        self.0 = hash;
    }
}

impl BuildHasher for NoHash {
    type Hasher = Self;
    #[inline]
    fn build_hasher(&self) -> Self::Hasher {
        Self(0)
    }
}
