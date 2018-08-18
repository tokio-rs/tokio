//! TODO: Dox

use {Error, Delay};
use clock::now;
use wheel::{self, Wheel};

use futures::{Future, Stream, Poll};
use slab::Slab;

use std::cmp;
use std::marker::PhantomData;
use std::time::{Duration, Instant};

/// TODO: Dox
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
    delay: Option<Delay>,

    /// Wheel polling state
    poll: wheel::Poll,

    /// Instant at which the timer starts
    start: Instant,
}

/// TOOD: Dox
#[derive(Debug)]
pub struct Entry<T> {
    /// The data stored in the queue
    data: T,

    /// The expiration time
    deadline: Instant,

    /// The key associated with the entry
    key: Key,
}

/// TODO: Dox
#[derive(Debug)]
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

    /// Next entry in the stack
    next: Option<usize>,

    /// Previous entry in the stac
    prev: Option<usize>,
}

/// Maximum number of entries the queue can handle
const MAX_ENTRIES: usize = (1 << 30) - 1;

impl<T> DelayQueue<T> {
    /// TODO: Dox
    pub fn new() -> DelayQueue<T> {
        DelayQueue {
            wheel: Wheel::new(),
            slab: Slab::new(),
            expired: Stack::default(),
            delay: None,
            poll: wheel::Poll::new(0),
            start: now(),
        }
    }

    /// TODO: Delete
    pub fn start(&self) -> Instant {
        self.start
    }

    /// TODO: Dox
    pub fn insert(&mut self, value: T, when: Instant) -> Key {
        assert!(self.slab.len() < MAX_ENTRIES, "max entries exceeded");

        // Normalize the deadline. Values cannot be set to expire in the past.
        let when = self.normalize_deadline(when);

        // Insert the value in the store
        let key = self.slab.insert(Data {
            inner: value,
            when,
            next: None,
            prev: None,
        });

        self.insert_idx(when, key);

        Key::new(key)
    }

    fn insert_idx(&mut self, when: u64, key: usize) {
        use self::wheel::{InsertError, Stack};

        // Register the deadline with the timer wheel
        match self.wheel.insert(when, key, &mut self.slab) {
            Ok(_) => {}
            Err((_, InsertError::Elapsed)) => {
                // The delay is already expired, store it in the expired queue
                self.expired.push(key, &mut self.slab);
            }
            Err((_, err)) => {
                panic!("invalid deadline; err={:?}", err)
            }
        }
    }

    /// TODO: Dox
    pub fn remove(&mut self, key: &Key) -> Entry<T> {
        self.wheel.remove(&key.index, &mut self.slab);
        let data = self.slab.remove(key.index);

        Entry {
            key: Key::new(key.index),
            data: data.inner,
            deadline: self.start + Duration::from_millis(data.when),
        }
    }

    /// TODO: Dox
    pub fn reset(&mut self, key: &Key, when: Instant) {
        self.wheel.remove(&key.index, &mut self.slab);

        // Normalize the deadline. Values cannot be set to expire in the past.
        let when = self.normalize_deadline(when);
        let old = self.start + Duration::from_millis(self.slab[key.index].when);


        self.slab[key.index].when = when;

        if let Some(ref mut delay) = self.delay {
            debug_assert!(old >= delay.deadline());

            if old == delay.deadline() {
                delay.reset(self.start + Duration::from_millis(when));
            }
        }

        self.insert_idx(when, key.index);
    }

    /// TODO: Dox
    pub fn clear(&mut self) {
        unimplemented!();
    }

    /// TODO: Dox
    pub fn new_with_capacity() -> DelayQueue<T> {
        unimplemented!();
    }

    /// TODO: Dox
    pub fn capacity(&self) -> usize {
        unimplemented!();
    }

    /// TODO: Dox
    pub fn reserve(&mut self, additional: usize) {
        drop(additional);
        unimplemented!();
    }

    /// TODO: Dox
    pub fn is_empty(&self) -> bool {
        unimplemented!();
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

            let deadline = match self.wheel.poll_at() {
                Some(poll_at) => {
                    self.start + Duration::from_millis(poll_at)
                }
                None => return Ok(None.into()),
            };

            self.delay = Some(Delay::new(deadline));
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
    type Item = Entry<T>;
    type Error = Error;

    fn poll(&mut self) -> Poll<Option<Self::Item>, Error> {
        let item = try_ready!(self.poll_idx())
            .map(|idx| {
                let data = self.slab.remove(idx);
                debug_assert!(data.next.is_none());
                debug_assert!(data.prev.is_none());

                Entry {
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

impl<T> Entry<T> {
    /// TODO: Dox
    pub fn get_ref(&self) -> &T {
        &self.data
    }

    /// TODO: Dox
    pub fn get_mut(&mut self) -> &mut T {
        &mut self.data
    }

    /// TODO: Dox
    pub fn into_inner(self) -> T {
        self.data
    }
}
