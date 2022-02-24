use std::{
    borrow::Borrow,
    collections::hash_map::{HashMap, RandomState},
    fmt,
    future::Future,
    hash::{BuildHasher, Hash},
};
use tokio::{
    runtime::Handle,
    task::{AbortHandle, JoinError, JoinSet, LocalSet},
};

pub struct JoinMap<K, V, S = RandomState> {
    aborts: HashMap<K, AbortHandle, S>,
    joins: JoinSet<(K, V)>,
}

impl<K, V> JoinMap<K, V> {
    /// Create a new `JoinMap`.
    pub fn new() -> Self {
        Self::with_hasher(RandomState::new())
    }
}

impl<K, V, S> JoinMap<K, V, S> {
    /// Creates an empty `JoinMap` which will use the given hash builder to hash
    /// keys.
    ///
    /// The created map has the default initial capacity.
    ///
    /// Warning: `hash_builder` is normally randomly generated, and
    /// is designed to allow HashMaps to be resistant to attacks that
    /// cause many collisions and very poor performance. Setting it
    /// manually using this function can expose a DoS attack vector.
    ///
    /// The `hash_builder` passed should implement the [`BuildHasher`] trait for
    /// the HashMap to be useful, see its documentation for details.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    /// use std::collections::hash_map::RandomState;
    ///
    /// let s = RandomState::new();
    /// let mut map = HashMap::with_hasher(s);
    /// map.insert(1, 2);
    /// ```
    #[inline]
    pub fn with_hasher(hash_builder: S) -> Self {
        Self {
            aborts: HashMap::with_hasher(hash_builder),
            joins: JoinSet::new(),
        }
    }

    /// Returns the number of tasks currently in the `JoinMap`.
    pub fn len(&self) -> usize {
        self.joins.len()
    }

    /// Returns whether the `JoinMap` is empty.
    pub fn is_empty(&self) -> bool {
        self.joins.is_empty()
    }
}

impl<K, V, S> JoinMap<K, V, S>
where
    K: Hash + Eq + Clone + 'static,
    V: 'static,
    S: BuildHasher,
{
    /// Spawn the provided task on the `JoinMap`, returning an [`AbortHandle`]
    /// that can be used to remotely cancel the task.
    ///
    /// # Panics
    ///
    /// This method panics if called outside of a Tokio runtime.
    ///
    /// [`AbortHandle`]: crate::task::AbortHandle
    pub fn spawn<F>(&mut self, key: K, task: F)
    where
        F: Future<Output = V>,
        F: Send + 'static,
        K: Send,
        V: Send,
    {
        let task = self.joins.spawn(mk_task(&key, task));
        self.insert(key, task)
    }

    /// Spawn the provided task on the provided runtime and store it in this
    /// `JoinMap` returning an [`AbortHandle`] that can be used to remotely
    /// cancel the task.
    ///
    /// [`AbortHandle`]: crate::task::AbortHandle
    pub fn spawn_on<F>(&mut self, key: K, task: F, handle: &Handle)
    where
        F: Future<Output = V>,
        F: Send + 'static,
        K: Send,
        V: Send,
    {
        let task = self.joins.spawn_on(mk_task(&key, task), handle);
        self.insert(key, task)
    }

    /// Spawn the provided task on the current [`LocalSet`] and store it in this
    /// `JoinMap`, returning an [`AbortHandle`]  that can be used to remotely
    /// cancel the task.
    ///
    /// # Panics
    ///
    /// This method panics if it is called outside of a `LocalSet`.
    ///
    /// [`LocalSet`]: crate::task::LocalSet
    /// [`AbortHandle`]: crate::task::AbortHandle
    pub fn spawn_local<F>(&mut self, key: K, task: F)
    where
        F: Future<Output = V>,
        F: 'static,
    {
        let task = self.joins.spawn_local(mk_task(&key, task));
        self.insert(key, task);
    }

    /// Spawn the provided task on the provided [`LocalSet`] and store it in
    /// this `JoinMap`, returning an [`AbortHandle`] that can be used to
    /// remotely cancel the task.
    ///
    /// [`LocalSet`]: crate::task::LocalSet
    /// [`AbortHandle`]: crate::task::AbortHandle
    pub fn spawn_local_on<F>(&mut self, key: K, task: F, local_set: &LocalSet)
    where
        F: Future<Output = V>,
        F: 'static,
    {
        let task = self.joins.spawn_local_on(mk_task(&key, task), local_set);
        self.insert(key, task)
    }

    fn insert(&mut self, key: K, abort: AbortHandle) {
        if let Some(prev) = self.aborts.insert(key, abort) {
            prev.abort();
        }
    }

    /// Waits until one of the tasks in the set completes and returns its output.
    ///
    /// Returns `None` if the set is empty.
    ///
    /// # Cancel Safety
    ///
    /// This method is cancel safe. If `join_one` is used as the event in a `tokio::select!`
    /// statement and some other branch completes first, it is guaranteed that no tasks were
    /// removed from this `JoinMap`.
    pub async fn join_one(&mut self) -> Result<Option<(K, V)>, JoinError> {
        match self.joins.join_one().await {
            Ok(Some((key, val))) => {
                self.aborts.remove(&key);
                Ok(Some((key, val)))
            }
            // XXX(eliza): should have a way to clean up "dead" abort handles
            // when a task panics or is cancelled by a runtime dropping, etc...
            res => res,
        }
    }

    /// Aborts all tasks and waits for them to finish shutting down.
    ///
    /// Calling this method is equivalent to calling [`abort_all`] and then calling [`join_one`] in
    /// a loop until it returns `Ok(None)`.
    ///
    /// This method ignores any panics in the tasks shutting down. When this call returns, the
    /// `JoinMap` will be empty.
    ///
    /// [`abort_all`]: fn@Self::abort_all
    /// [`join_one`]: fn@Self::join_one
    pub async fn shutdown(&mut self) {
        self.abort_all();
        while self.join_one().await.transpose().is_some() {}
    }

    pub fn abort<Q: ?Sized>(&mut self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        match self.aborts.remove(key) {
            Some(task) => {
                task.abort();
                true
            }
            None => false,
        }
    }

    pub fn contains_task<Q: ?Sized>(&mut self, key: &Q) -> bool
    where
        Q: Hash + Eq,
        K: Borrow<Q>,
    {
        self.aborts.contains_key(key)
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
        self.joins.abort_all();
        self.aborts.clear();
    }

    /// Removes all tasks from this `JoinMap` without aborting them.
    ///
    /// The tasks removed by this call will continue to run in the background even if the `JoinMap`
    /// is dropped.
    pub fn detach_all(&mut self) {
        self.joins.detach_all()
    }
}

impl<K: fmt::Debug, V, S> fmt::Debug for JoinMap<K, V, S> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        struct KeySet<'a, K, V, S>(&'a JoinMap<K, V, S>);
        impl<K: fmt::Debug, V, S> fmt::Debug for KeySet<'_, K, V, S> {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.debug_set().entries(self.0.aborts.keys()).finish()
            }
        }
        f.debug_struct("JoinMap")
            .field("keys", &KeySet(self))
            .finish()
    }
}

impl<K, V> Default for JoinMap<K, V> {
    fn default() -> Self {
        Self::new()
    }
}

fn mk_task<K: Clone, F, V>(key: &K, task: F) -> impl Future<Output = (K, V)>
where
    F: Future<Output = V>,
{
    let key = key.clone();
    async move { (key, task.await) }
}
