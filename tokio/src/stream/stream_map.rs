use crate::stream::Stream;

use std::borrow::Borrow;
use std::hash::Hash;
use std::pin::Pin;
use std::task::{Context, Poll};

/// Combine many streams into one, indexing each source stream with a unique
/// key.
///
/// `StreamMap` is similar to [`StreamExt::merge`] in that it combines source
/// streams into a single merged stream that yields values in the order that
/// they arrive from the source streams. However, `StreamMap` has a lot more
/// flexibility in usage patterns.
///
/// `StreamMap` can:
///
/// * Merge an arbitrary number of streams.
/// * Track which source stream the value was received from.
/// * Handle inserting and removing streams from the set of managed streams at
///   any point during iteration.
///
/// All source streams held by `StreamMap` are indexed using a key. This key is
/// included with the value when a source stream yields a value. The key is also
/// used to remove the stream from the `StreamMap` before the stream has
/// completed streaming.
///
/// # `Unpin`
///
/// Because the `StreamMap` API moves streams during runtime, both streams and
/// keys must be `Unpin`. In order to insert a `!Unpin` stream into a
/// `StreamMap`, use [`pin!`] to pin the stream to the stack or [`Box::pin`] to
/// pin the stream in the heap.
///
/// # Implementation
///
/// `StreamMap` is backed by a `Vec<(K, V)>`. There is no guarantee that this
/// internal implementation detail will persist in future versions, but it is
/// important to know the runtime implications. In general, `StreamMap` works
/// best with a "smallish" number of streams as all entries are scanned on
/// insert, remove, and polling. In cases where a large number of streams need
/// to be merged, it may be advisable to use tasks sending values on a shared
/// [`mpsc`] channel.
///
/// [`StreamExt::merge`]: crate::stream::StreamExt::merge
/// [`mpsc`]: crate::sync::mpsc
/// [`pin!`]: macro@pin
/// [`Box::pin`]: std::boxed::Box::pin
///
/// # Examples
///
/// Merging two streams, then remove them after receiving the first value
///
/// ```
/// use tokio::stream::{StreamExt, StreamMap};
/// use tokio::sync::mpsc;
///
/// #[tokio::main]
/// async fn main() {
///     let (mut tx1, rx1) = mpsc::channel(10);
///     let (mut tx2, rx2) = mpsc::channel(10);
///
///     tokio::spawn(async move {
///         tx1.send(1).await.unwrap();
///
///         // This value will never be received. The send may or may not return
///         // `Err` depending on if the remote end closed first or not.
///         let _ = tx1.send(2).await;
///     });
///
///     tokio::spawn(async move {
///         tx2.send(3).await.unwrap();
///         let _ = tx2.send(4).await;
///     });
///
///     let mut map = StreamMap::new();
///
///     // Insert both streams
///     map.insert("one", rx1);
///     map.insert("two", rx2);
///
///     // Read twice
///     for _ in 0..2 {
///         let (key, val) = map.next().await.unwrap();
///
///         if key == "one" {
///             assert_eq!(val, 1);
///         } else {
///             assert_eq!(val, 3);
///         }
///
///         // Remove the stream to prevent reading the next value
///         map.remove(key);
///     }
/// }
/// ```
///
/// This example models a read-only client to a chat system with channels. The
/// client sends commands to join and leave channels. `StreamMap` is used to
/// manage active channel subscriptions.
///
/// For simplicity, messages are displayed with `println!`, but they could be
/// sent to the client over a socket.
///
/// ```no_run
/// use tokio::stream::{Stream, StreamExt, StreamMap};
///
/// enum Command {
///     Join(String),
///     Leave(String),
/// }
///
/// fn commands() -> impl Stream<Item = Command> {
///     // Streams in user commands by parsing `stdin`.
/// # tokio::stream::pending()
/// }
///
/// // Join a channel, returns a stream of messages received on the channel.
/// fn join(channel: &str) -> impl Stream<Item = String> + Unpin {
///     // left as an exercise to the reader
/// # tokio::stream::pending()
/// }
///
/// #[tokio::main]
/// async fn main() {
///     let mut channels = StreamMap::new();
///
///     // Input commands (join / leave channels).
///     let cmds = commands();
///     tokio::pin!(cmds);
///
///     loop {
///         tokio::select! {
///             Some(cmd) = cmds.next() => {
///                 match cmd {
///                     Command::Join(chan) => {
///                         // Join the channel and add it to the `channels`
///                         // stream map
///                         let msgs = join(&chan);
///                         channels.insert(chan, msgs);
///                     }
///                     Command::Leave(chan) => {
///                         channels.remove(&chan);
///                     }
///                 }
///             }
///             Some((chan, msg)) = channels.next() => {
///                 // Received a message, display it on stdout with the channel
///                 // it originated from.
///                 println!("{}: {}", chan, msg);
///             }
///             // Both the `commands` stream and the `channels` stream are
///             // complete. There is no more work to do, so leave the loop.
///             else => break,
///         }
///     }
/// }
/// ```
#[derive(Debug, Default)]
pub struct StreamMap<K, V> {
    /// Streams stored in the map
    entries: Vec<(K, V)>,
}

impl<K, V> StreamMap<K, V> {
    /// Creates an empty `StreamMap`.
    ///
    /// The stream map is initially created with a capacity of `0`, so it will
    /// not allocate until it is first inserted into.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, Pending};
    ///
    /// let map: StreamMap<&str, Pending<()>> = StreamMap::new();
    /// ```
    pub fn new() -> StreamMap<K, V> {
        StreamMap { entries: vec![] }
    }

    /// Creates an empty `StreamMap` with the specified capacity.
    ///
    /// The stream map will be able to hold at least `capacity` elements without
    /// reallocating. If `capacity` is 0, the stream map will not allocate.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, Pending};
    ///
    /// let map: StreamMap<&str, Pending<()>> = StreamMap::with_capacity(10);
    /// ```
    pub fn with_capacity(capacity: usize) -> StreamMap<K, V> {
        StreamMap {
            entries: Vec::with_capacity(capacity),
        }
    }

    /// Returns an iterator visiting all keys in arbitrary order.
    ///
    /// The iterator element type is &'a K.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut map = StreamMap::new();
    ///
    /// map.insert("a", pending::<i32>());
    /// map.insert("b", pending());
    /// map.insert("c", pending());
    ///
    /// for key in map.keys() {
    ///     println!("{}", key);
    /// }
    /// ```
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.iter().map(|(k, _)| k)
    }

    /// An iterator visiting all values in arbitrary order.
    ///
    /// The iterator element type is &'a V.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut map = StreamMap::new();
    ///
    /// map.insert("a", pending::<i32>());
    /// map.insert("b", pending());
    /// map.insert("c", pending());
    ///
    /// for stream in map.values() {
    ///     println!("{:?}", stream);
    /// }
    /// ```
    pub fn values(&self) -> impl Iterator<Item = &V> {
        self.entries.iter().map(|(_, v)| v)
    }

    /// An iterator visiting all values mutably in arbitrary order.
    ///
    /// The iterator element type is &'a mut V.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut map = StreamMap::new();
    ///
    /// map.insert("a", pending::<i32>());
    /// map.insert("b", pending());
    /// map.insert("c", pending());
    ///
    /// for stream in map.values_mut() {
    ///     println!("{:?}", stream);
    /// }
    /// ```
    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut V> {
        self.entries.iter_mut().map(|(_, v)| v)
    }

    /// Returns the number of streams the map can hold without reallocating.
    ///
    /// This number is a lower bound; the `StreamMap` might be able to hold
    /// more, but is guaranteed to be able to hold at least this many.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, Pending};
    ///
    /// let map: StreamMap<i32, Pending<()>> = StreamMap::with_capacity(100);
    /// assert!(map.capacity() >= 100);
    /// ```
    pub fn capacity(&self) -> usize {
        self.entries.capacity()
    }

    /// Returns the number of streams in the map.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut a = StreamMap::new();
    /// assert_eq!(a.len(), 0);
    /// a.insert(1, pending::<i32>());
    /// assert_eq!(a.len(), 1);
    /// ```
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Returns `true` if the map contains no elements.
    ///
    /// # Examples
    ///
    /// ```
    /// use std::collections::HashMap;
    ///
    /// let mut a = HashMap::new();
    /// assert!(a.is_empty());
    /// a.insert(1, "a");
    /// assert!(!a.is_empty());
    /// ```
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Clears the map, removing all key-stream pairs. Keeps the allocated
    /// memory for reuse.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut a = StreamMap::new();
    /// a.insert(1, pending::<i32>());
    /// a.clear();
    /// assert!(a.is_empty());
    /// ```
    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Insert a key-stream pair into the map.
    ///
    /// If the map did not have this key present, `None` is returned.
    ///
    /// If the map did have this key present, the new `stream` replaces the old
    /// one and the old stream is returned.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut map = StreamMap::new();
    ///
    /// assert!(map.insert(37, pending::<i32>()).is_none());
    /// assert!(!map.is_empty());
    ///
    /// map.insert(37, pending());
    /// assert!(map.insert(37, pending()).is_some());
    /// ```
    pub fn insert(&mut self, k: K, stream: V) -> Option<V>
    where
        K: Hash + Eq,
    {
        let ret = self.remove(&k);
        self.entries.push((k, stream));

        ret
    }

    /// Removes a key from the map, returning the stream at the key if the key was previously in the map.
    ///
    /// The key may be any borrowed form of the map's key type, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut map = StreamMap::new();
    /// map.insert(1, pending::<i32>());
    /// assert!(map.remove(&1).is_some());
    /// assert!(map.remove(&1).is_none());
    /// ```
    pub fn remove<Q: ?Sized>(&mut self, k: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        for i in 0..self.entries.len() {
            if self.entries[i].0.borrow() == k {
                return Some(self.entries.swap_remove(i).1);
            }
        }

        None
    }

    /// Returns `true` if the map contains a stream for the specified key.
    ///
    /// The key may be any borrowed form of the map's key type, but `Hash` and
    /// `Eq` on the borrowed form must match those for the key type.
    ///
    /// # Examples
    ///
    /// ```
    /// use tokio::stream::{StreamMap, pending};
    ///
    /// let mut map = StreamMap::new();
    /// map.insert(1, pending::<i32>());
    /// assert_eq!(map.contains_key(&1), true);
    /// assert_eq!(map.contains_key(&2), false);
    /// ```
    pub fn contains_key<Q: ?Sized>(&self, k: &Q) -> bool
    where
        K: Borrow<Q>,
        Q: Hash + Eq,
    {
        for i in 0..self.entries.len() {
            if self.entries[i].0.borrow() == k {
                return true;
            }
        }

        false
    }
}

impl<K, V> StreamMap<K, V>
where
    K: Unpin,
    V: Stream + Unpin,
{
    /// Polls the next value, includes the vec entry index
    fn poll_next_entry(&mut self, cx: &mut Context<'_>) -> Poll<Option<(usize, V::Item)>> {
        use Poll::*;

        let start = crate::util::thread_rng_n(self.entries.len() as u32) as usize;
        let mut idx = start;

        for _ in 0..self.entries.len() {
            let (_, stream) = &mut self.entries[idx];

            match Pin::new(stream).poll_next(cx) {
                Ready(Some(val)) => return Ready(Some((idx, val))),
                Ready(None) => {
                    // Remove the entry
                    self.entries.swap_remove(idx);

                    // Check if this was the last entry, if so the cursor needs
                    // to wrap
                    if idx == self.entries.len() {
                        idx = 0;
                    } else if idx < start && start <= self.entries.len() {
                        // The stream being swapped into the current index has
                        // already been polled, so skip it.
                        idx = idx.wrapping_add(1) % self.entries.len();
                    }
                }
                Pending => {
                    idx = idx.wrapping_add(1) % self.entries.len();
                }
            }
        }

        // If the map is empty, then the stream is complete.
        if self.entries.is_empty() {
            Ready(None)
        } else {
            Pending
        }
    }
}

impl<K, V> Stream for StreamMap<K, V>
where
    K: Clone + Unpin,
    V: Stream + Unpin,
{
    type Item = (K, V::Item);

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Some((idx, val)) = ready!(self.poll_next_entry(cx)) {
            let key = self.entries[idx].0.clone();
            Poll::Ready(Some((key, val)))
        } else {
            Poll::Ready(None)
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let mut ret = (0, Some(0));

        for (_, stream) in &self.entries {
            let hint = stream.size_hint();

            ret.0 += hint.0;

            match (ret.1, hint.1) {
                (Some(a), Some(b)) => ret.1 = Some(a + b),
                (Some(_), None) => ret.1 = None,
                _ => {}
            }
        }

        ret
    }
}
