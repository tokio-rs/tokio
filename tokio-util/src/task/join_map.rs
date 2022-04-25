use tokio::runtime::Handle;
use tokio::task::{JoinError, AbortHandle, LocalSet, JoinSet, Id};
use std::borrow::Borrow;
use std::collections::hash_map::RandomState;
use hashbrown::HashMap;
use hashbrown::hash_map::RawEntryMut;
use std::future::Future;
use std::hash::{BuildHasher, Hash, Hasher};

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
#[derive(Debug)]
pub struct JoinMap<K, V, S = RandomState> {
    tasks_by_key: HashMap<Key<K>, AbortHandle, S>,
    hashes_by_task: HashMap<Id, u64, S>,

    /// The set of tasks spawned on the `JoinMap`.
    ///
    /// Each `IdleNotifiedSet` entry contains the hash of the task's key, to
    /// allow looking the key up when the task completes.
    task_set: JoinSet<V>,
}

/// A `JoinMap` key.
#[derive(Debug, Hash, PartialEq, Eq)]
struct Key<K> {
    key: K,
    id: Id,
}

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

impl<K, V, S: Clone> JoinMap<K, V, S> {
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
            tasks_by_key: HashMap::with_capacity_and_hasher(capacity, hash_builder.clone()),
            hashes_by_task: HashMap::with_capacity_and_hasher(capacity, hash_builder),
            task_set: JoinSet::new(),
        }
    }

    /// Returns the number of tasks currently in the `JoinMap`.
    pub fn len(&self) -> usize {
        let len = self.tasks_by_key.len();
        debug_assert_eq!(len, self.hashes_by_task.len());
        len
    }

    /// Returns whether the `JoinMap` is empty.
    pub fn is_empty(&self) -> bool {
        let empty = self.tasks_by_key.is_empty();
        debug_assert_eq!(empty, self.hashes_by_task.is_empty());
        empty
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
        let capacity = self.tasks_by_key.capacity();
        debug_assert_eq!(capacity, self.hashes_by_task.capacity());
        capacity
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
        let task = self.task_set.spawn(task);
        self.insert(key, task)
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
        let task = self.task_set.spawn_on(task, handle);
        self.insert(key, task);
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
        let task = self.task_set.spawn_local(task);
        self.insert(key, task);
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
        let task =  self.task_set.spawn_local_on(task, local_set);
        self.insert(key, task)
    }

    fn insert(&mut self, key: K, abort: AbortHandle) {
        let hash = self.hash(&key);
        let id = abort.id();
        let map_key = Key { id: id.clone(), key };
        let entry = self.tasks_by_key.raw_entry_mut().from_hash(hash, |k| k.key == map_key.key);
        match entry {
            RawEntryMut::Occupied(mut occ) => {
                let Key { id: prev_id, .. } = occ.insert_key(map_key);
                occ.insert(abort).abort();
                let _prev_hash = self.hashes_by_task.remove(&prev_id);
                debug_assert_eq!(Some(hash), _prev_hash);
            },
            RawEntryMut::Vacant(vac) => { 
                vac.insert(map_key, abort);
            }
        };

        let _prev = self.hashes_by_task.insert(id, hash);
        debug_assert!(_prev.is_none(), "no prior task should have had the same ID");
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
        let (res, id) = match self.task_set.join_one_with_id().await {
            Ok(task) => {
                let (id, output) = task?;
                (Ok(output), id)
            },
            Err(e) => {
                let id = e.id();
                (Err(e), id)
            }
        };
        let key = self.remove_by_id(id)?;
        Some((key, res))
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
    pub fn abort<Q: ?Sized>(&mut self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        match self.remove_by_key(key) {
            Some(handle) => { handle.abort(); true }
            None => false,
        }
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
        // let key_set = &mut self.key_set;
        // let joins = &mut self.task_set;
        // // Note: this method iterates over the key set *without* removing any
        // // entries, so that the keys from aborted tasks can be returned when
        // // polling the `JoinMap`.
        // for entry in key_set.iter().filter(|entry| predicate(&entry.key)) {
        //     if let Some(mut entry) = joins.entry_mut(entry.task.entry.clone()) {
        //         entry.with_value_and_context(|(_, jh), _| jh.abort());
        //     }
        // }
        todo!("eliza")
    }

    /// Returns `true` if this `JoinMap` contains a task for the provided key.
    ///
    /// If the task has completed, but its output hasn't yet been consumed by a
    /// call to [`join_one`], this method will still return `true`.
    ///
    /// [`join_one`]: fn@Self::join_one
    pub fn contains_key<Q: ?Sized>(&self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        self.get_by_key(key).is_some()
    }

    pub fn contains_task(&self, task: &Id) -> bool {
        self.get_by_id(task).is_some()
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
        self.tasks_by_key.reserve(additional);
        self.hashes_by_task.reserve(additional);
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
        self.hashes_by_task.shrink_to_fit();
        self.tasks_by_key.shrink_to_fit();
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
        self.hashes_by_task.shrink_to(min_capacity);
        self.tasks_by_key.shrink_to(min_capacity)
    }

    fn get_by_key<'map, Q: ?Sized>(&'map self, key: &Q) -> Option<(&'map Key<K>, &'map AbortHandle)>
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        let hash = self.hash(key);
        self.tasks_by_key.raw_entry().from_hash(hash, |k| k.key.borrow() == key)
    }

    fn get_by_id<'map>(&'map self, id: &Id) -> Option<(&'map Key<K>, &'map AbortHandle)> {
        let hash = self.hashes_by_task.get(id)?;
        self.tasks_by_key.raw_entry().from_hash(*hash, |k| &k.id == id)
    }

    fn remove_by_id(&mut self, id: Id) -> Option<K> {
        let hash = self.hashes_by_task.remove(&id)?;
        let entry = self.tasks_by_key.raw_entry_mut().from_hash(hash, |k| k.id == id);
        let (Key { id: _key_id, key }, handle) = match entry {
            RawEntryMut::Occupied(entry) => entry.remove_entry(),
            _ => return None,
        };
        debug_assert_eq!(_key_id, id);
        debug_assert_eq!(id, handle.id());
        self.hashes_by_task.remove(&id);
        Some(key)
    }

    fn remove_by_key<Q: ?Sized>(&mut self, key: &Q) -> Option<AbortHandle>
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        let hash = self.hash(key);
        let entry = self.tasks_by_key.raw_entry_mut().from_hash(hash, |k| k.key.borrow() == key);
        let handle = match entry {
            RawEntryMut::Occupied(entry) => entry.remove(),
            _ => return None,
        };
        self.hashes_by_task.remove(&handle.id());
        Some(handle)
    }

    fn hash<Q: ?Sized>(&self, key: &Q) -> u64
    where
        Q: Hash,
    {
        let mut hasher = self.tasks_by_key.hasher().build_hasher();
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
        self.task_set.abort_all()
    }

    /// Removes all tasks from this `JoinMap` without aborting them.
    ///
    /// The tasks removed by this call will continue to run in the background even if the `JoinMap`
    /// is dropped. They may still be aborted by key.
    pub fn detach_all(&mut self) {
        self.task_set.detach_all()
    }
}


// impl<K: fmt::Debug + 'static, V: 'static, S> fmt::Debug for JoinMap<K, V, S> {
//     fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//         // debug print the keys in this `JoinMap`.
//         struct KeySet<'a, K, V, S>(&'a JoinMap<K, V, S>);
//         impl<K: fmt::Debug + 'static, V: 'static, S> fmt::Debug for KeySet<'_, K, V, S> {
//             fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
//                 f.debug_set()
//                     .entries(self.0.key_set.iter().map(|entry| &entry.key))
//                     .finish()
//             }
//         }

//         f.debug_struct("JoinMap")
//             .field("key_set", &KeySet(self))
//             .finish()
//     }
// }

impl<K, V> Default for JoinMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}