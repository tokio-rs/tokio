//! A slot in memory for communicating between a producer and a consumer.
//!
//! This module contains an implementation detail of this library for a type
//! which is only intended to be shared between one consumer and one producer of
//! a value. It is unlikely that this module will survive stabilization of this
//! library, so it is not recommended to rely on it.

#![allow(dead_code)] // imported in a few places

use std::prelude::v1::*;
use std::sync::atomic::{AtomicUsize, Ordering};

use lock::Lock;

/// A slot in memory intended to represent the communication channel between one
/// producer and one consumer.
///
/// Each slot contains space for a piece of data of type `T`, and can have
/// callbacks registered to run when the slot is either full or empty.
///
/// Slots are only intended to be shared between exactly one producer and
/// exactly one consumer. If there are multiple concurrent producers or
/// consumers then this is still memory safe but will have unpredictable results
/// (and maybe panics). Note that this does not require that the "consumer" is
/// the same for the entire lifetime of a slot, simply that there is only one
/// consumer at a time.
///
/// # Registering callbacks
///
/// [`on_empty`](#method.on_empty) registers a callback to run when the slot
/// becomes empty, and [`on_full`](#method.on_full) registers one to run when it
/// becomes full. In both cases, the callback will run immediately if possible.
///
/// At most one callback can be registered at any given time: it is an error to
/// attempt to register a callback with `on_full` if one is currently registered
/// via `on_empty`, or any other combination.
///
/// # Cancellation
///
/// Registering a callback returns a `Token` which can be used to
/// [`cancel`](#method.cancel) the callback. Only callbacks that have not yet
/// started running can be canceled. Canceling a callback that has already run
/// is not an error, and `cancel` does not signal whether or not the callback
/// was actually canceled to the caller.
pub struct Slot<T> {
    // The purpose of this data type is to communicate when a value becomes
    // available and coordinate between a producer and consumer about that
    // value. Slots end up being at the core of many futures as they handle
    // values transferring between producers and consumers, which means that
    // they can never block.
    //
    // As a result, this `Slot` is a lock-free implementation in terms of not
    // actually blocking at any point in time. The `Lock` types are
    // half-optional and half-not-optional. They aren't actually mutexes as they
    // only support a `try_lock` operation, and all the methods below ensure
    // that progress can always be made without blocking.
    //
    // The `state` variable keeps track of the state of this slot, while the
    // other fields here are just the payloads of the slot itself. Note that the
    // exact bits of `state` are typically wrapped up in a `State` for
    // inspection (see below).
    state: AtomicUsize,
    slot: Lock<Option<T>>,
    on_full: Lock<Option<Box<FnBox<T>>>>,
    on_empty: Lock<Option<(Box<FnBox2<T>>, Option<T>)>>,
}

/// Error value returned from erroneous calls to `try_produce`, which contains
/// the value that was passed to `try_produce`.
#[derive(Debug, PartialEq)]
pub struct TryProduceError<T>(T);

/// Error value returned from erroneous calls to `try_consume`.
#[derive(Debug, PartialEq)]
pub struct TryConsumeError(());

/// Error value returned from erroneous calls to `on_full`.
#[derive(Debug, PartialEq)]
pub struct OnFullError(());

/// Error value returned from erroneous calls to `on_empty`.
#[derive(Debug, PartialEq)]
pub struct OnEmptyError(());

/// A `Token` represents a registered callback, and can be used to cancel the callback.
#[derive(Clone, Copy)]
pub struct Token(usize);

// Slot state: the lowest 3 bits are flags; the remaining bits are used to
// store the `Token` for the currently registered callback. The special token
// value 0 means no callback is registered.
//
// The flags are:
//   - `DATA`: the `Slot` contains a value
//   - `ON_FULL`: the `Slot` has an `on_full` callback registered
//   - `ON_EMPTY`: the `Slot` has an `on_empty` callback registered
struct State(usize);

const DATA: usize = 1 << 0;
const ON_FULL: usize = 1 << 1;
const ON_EMPTY: usize = 1 << 2;
const STATE_BITS: usize = 3;
const STATE_MASK: usize = (1 << STATE_BITS) - 1;

fn _is_send<T: Send>() {}
fn _is_sync<T: Send>() {}

fn _assert() {
    _is_send::<Slot<i32>>();
    _is_sync::<Slot<u32>>();
}

impl<T> Slot<T> {
    /// Creates a new `Slot` containing `val`, which may be `None` to create an
    /// empty `Slot`.
    pub fn new(val: Option<T>) -> Slot<T> {
        Slot {
            state: AtomicUsize::new(if val.is_some() {DATA} else {0}),
            slot: Lock::new(val),
            on_full: Lock::new(None),
            on_empty: Lock::new(None),
        }
    }

    /// Attempts to store `t` in the slot.
    ///
    /// This method can only be called by the one consumer working on this
    /// `Slot`. Concurrent calls to this method or `on_empty` will result in
    /// panics or possibly errors.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the slot is already full. The value you attempted to
    /// store is included in the error value.
    ///
    /// # Panics
    ///
    /// This method will panic if called concurrently with `try_produce` or
    /// `on_empty`, or if `on_empty` has been called previously but the callback
    /// hasn't fired.
    pub fn try_produce(&self, t: T) -> Result<(), TryProduceError<T>> {
        // First up, let's take a look at our current state. Of our three flags,
        // we check a few:
        //
        // * DATA - if this is set, then the production fails as a value has
        //          already been produced and we're not ready to receive it yet.
        // * ON_EMPTY - this should never be set as it indicates a contract
        //              violation as the producer already registered interest in
        //              a value but the callback wasn't fired.
        // * ON_FULL - doesn't matter in this use case, we don't check it as
        //             either state is valid.
        let mut state = State(self.state.load(Ordering::SeqCst));
        assert!(!state.flag(ON_EMPTY));
        if state.flag(DATA) {
            return Err(TryProduceError(t))
        }

        // Ok, so we've determined that our state is either `ON_FULL` or `0`, in
        // both cases we're going to store our data into our slot. This should
        // always succeed as access to `slot` is gated on the `DATA` flag being
        // set on the consumer side (which isn't set) and there should only be
        // one producer.
        let mut slot = self.slot.try_lock().expect("interference with consumer?");
        assert!(slot.is_none());
        *slot = Some(t);
        drop(slot);

        // Next, we update our state with `DATA` to say that something is
        // available, and we also unset `ON_FULL` because we'll invoke the
        // callback if it's available.
        loop {
            assert!(!state.flag(ON_EMPTY));
            let new_state = state.set_flag(DATA, true).set_flag(ON_FULL, false);
            let old = self.state.compare_and_swap(state.0,
                                                  new_state.0,
                                                  Ordering::SeqCst);
            if old == state.0 {
                break
            }
            state.0 = old;
        }

        // If our previous state we transitioned from indicates that it has an
        // on-full callback, we call that callback here. There's a few unwraps
        // here that should never fail because the consumer shouldn't be placing
        // another callback here and there shouldn't be any other producers as
        // well.
        if state.flag(ON_FULL) {
            let cb = self.on_full.try_lock().expect("interference2")
                                 .take().expect("ON_FULL but no callback");
            cb.call_box(self);
        }
        Ok(())
    }

    /// Registers `f` as a callback to run when the slot becomes empty.
    ///
    /// The callback will run immediately if the slot is already empty. Returns
    /// a token that can be used to cancel the callback. This method is to be
    /// called by the producer, and it is illegal to call this method
    /// concurrently with either `on_empty` or `try_produce`.
    ///
    /// # Panics
    ///
    /// Panics if another callback was already registered via `on_empty` or
    /// `on_full`, or if this value is called concurrently with other producer
    /// methods.
    pub fn on_empty<F>(&self, item: Option<T>, f: F) -> Token
        where F: FnOnce(&Slot<T>, Option<T>) + Send + 'static
    {
        // First up, as usual, take a look at our state. Of the three flags we
        // check two:
        //
        // * DATA - if set, we keep going, but if unset we're done as there's no
        //          data and we're already empty.
        // * ON_EMPTY - this should be impossible as it's a contract violation
        //              to call this twice or concurrently.
        // * ON_FULL - it's illegal to have both an empty and a full callback
        //             simultaneously, so we check this just after we ensure
        //             there's data available. If there's data there should not
        //             be a full callback as it should have been called.
        let mut state = State(self.state.load(Ordering::SeqCst));
        assert!(!state.flag(ON_EMPTY));
        if !state.flag(DATA) {
            f(self, item);
            return Token(0)
        }
        assert!(!state.flag(ON_FULL));

        // At this point we've precisely determined that our state is `DATA` and
        // all other flags are unset. We're cleared for landing in initializing
        // the `on_empty` slot so we store our callback here.
        let mut slot = self.on_empty.try_lock().expect("on_empty interference");
        assert!(slot.is_none());
        *slot = Some((Box::new(f), item));
        drop(slot);

        // In this loop, we transition ourselves from the `DATA` state to a
        // state which has the on empty flag state. Note that we also increase
        // the token of this state as we're registering a new callback.
        loop {
            assert!(state.flag(DATA));
            assert!(!state.flag(ON_FULL));
            assert!(!state.flag(ON_EMPTY));
            let new_state = state.set_flag(ON_EMPTY, true)
                                 .set_token(state.token() + 1);
            let old = self.state.compare_and_swap(state.0,
                                                  new_state.0,
                                                  Ordering::SeqCst);

            // If we succeeded in the CAS, then we're done and our token is
            // valid.
            if old == state.0 {
                return Token(new_state.token())
            }
            state.0 = old;

            // If we failed the CAS but the data was taken in the meantime we
            // abort our attempt to set on-empty and call the callback
            // immediately. Note that the on-empty flag was never set, so it
            // should still be there and should be available to take.
            if !state.flag(DATA) {
                let cb = self.on_empty.try_lock().expect("on_empty interference2")
                                      .take().expect("on_empty not empty??");
                let (cb, item) = cb;
                cb.call_box(self, item);
                return Token(0)
            }
        }
    }

    /// Attempts to consume the value stored in the slot.
    ///
    /// This method can only be called by the one consumer of this slot, and
    /// cannot be called concurrently with `try_consume` or `on_full`.
    ///
    /// # Errors
    ///
    /// Returns `Err` if the slot is already empty.
    ///
    /// # Panics
    ///
    /// This method will panic if called concurrently with `try_consume` or
    /// `on_full`, or otherwise show weird behavior.
    pub fn try_consume(&self) -> Result<T, TryConsumeError> {
        // The implementation of this method is basically the same as
        // `try_produce` above, it's just the opposite of all the operations.
        let mut state = State(self.state.load(Ordering::SeqCst));
        assert!(!state.flag(ON_FULL));
        if !state.flag(DATA) {
            return Err(TryConsumeError(()))
        }
        let mut slot = self.slot.try_lock().expect("interference with producer?");
        let val = slot.take().expect("DATA but not data");
        drop(slot);

        loop {
            assert!(!state.flag(ON_FULL));
            let new_state = state.set_flag(DATA, false).set_flag(ON_EMPTY, false);
            let old = self.state.compare_and_swap(state.0,
                                                  new_state.0,
                                                  Ordering::SeqCst);
            if old == state.0 {
                break
            }
            state.0 = old;
        }
        assert!(!state.flag(ON_FULL));
        if state.flag(ON_EMPTY) {
            let cb = self.on_empty.try_lock().expect("interference3")
                                  .take().expect("ON_EMPTY but no callback");
            let (cb, item) = cb;
            cb.call_box(self, item);
        }
        Ok(val)
    }

    /// Registers `f` as a callback to run when the slot becomes full.
    ///
    /// The callback will run immediately if the slot is already full. Returns a
    /// token that can be used to cancel the callback.
    ///
    /// This method is to be called by the consumer.
    ///
    /// # Panics
    ///
    /// Panics if another callback was already registered via `on_empty` or
    /// `on_full` or if called concurrently with `on_full` or `try_consume`.
    pub fn on_full<F>(&self, f: F) -> Token
        where F: FnOnce(&Slot<T>) + Send + 'static
    {
        // The implementation of this method is basically the same as
        // `on_empty` above, it's just the opposite of all the operations.
        let mut state = State(self.state.load(Ordering::SeqCst));
        assert!(!state.flag(ON_FULL));
        if state.flag(DATA) {
            f(self);
            return Token(0)
        }
        assert!(!state.flag(ON_EMPTY));

        let mut slot = self.on_full.try_lock().expect("on_full interference");
        assert!(slot.is_none());
        *slot = Some(Box::new(f));
        drop(slot);

        loop {
            assert!(!state.flag(DATA));
            assert!(!state.flag(ON_EMPTY));
            assert!(!state.flag(ON_FULL));
            let new_state = state.set_flag(ON_FULL, true)
                                 .set_token(state.token() + 1);
            let old = self.state.compare_and_swap(state.0,
                                                  new_state.0,
                                                  Ordering::SeqCst);
            if old == state.0 {
                return Token(new_state.token())
            }
            state.0 = old;

            if state.flag(DATA) {
                let cb = self.on_full.try_lock().expect("on_full interference2")
                                      .take().expect("on_full not full??");
                cb.call_box(self);
                return Token(0)
            }
        }
    }

    /// Cancels the callback associated with `token`.
    ///
    /// Canceling a callback that has already started running, or has already
    /// run will do nothing, and is not an error. See
    /// [Cancellation](#cancellation).
    ///
    /// # Panics
    ///
    /// This method may cause panics if it is called concurrently with
    /// `on_empty` or `on_full`, depending on which callback is being canceled.
    pub fn cancel(&self, token: Token) {
        // Tokens with a value of "0" are sentinels which don't actually do
        // anything.
        let token = token.0;
        if token == 0 {
            return
        }

        let mut state = State(self.state.load(Ordering::SeqCst));
        loop {
            // If we've moved on to a different token, then we're guaranteed
            // that our token won't show up again, so we can return immediately
            // as our closure has likely already run (or been previously
            // canceled).
            if state.token() != token {
                return
            }

            // If our token matches, then let's see if we're cancelling either
            // the on-full or on-empty callbacks. It's illegal to have them both
            // registered, so we only need to look at one.
            //
            // If neither are set then the token has probably already run, so we
            // just continue along our merry way and don't worry.
            let new_state = if state.flag(ON_FULL) {
                assert!(!state.flag(ON_EMPTY));
                state.set_flag(ON_FULL, false)
            } else if state.flag(ON_EMPTY) {
                assert!(!state.flag(ON_FULL));
                state.set_flag(ON_EMPTY, false)
            } else {
                return
            };
            let old = self.state.compare_and_swap(state.0,
                                                  new_state.0,
                                                  Ordering::SeqCst);
            if old == state.0 {
                break
            }
            state.0 = old;
        }

        // Figure out which callback we just canceled, and now that the flag is
        // unset we should own the callback to clear it.

        if state.flag(ON_FULL) {
            let cb = self.on_full.try_lock().expect("on_full interference3")
                                 .take().expect("on_full not full??");
            drop(cb);
        } else {
            let cb = self.on_empty.try_lock().expect("on_empty interference3")
                                  .take().expect("on_empty not empty??");
            drop(cb);
        }
    }
}

impl<T> TryProduceError<T> {
    /// Extracts the value that was attempted to be produced.
    pub fn into_inner(self) -> T {
        self.0
    }
}

trait FnBox<T>: Send {
    fn call_box(self: Box<Self>, other: &Slot<T>);
}

impl<T, F> FnBox<T> for F
    where F: FnOnce(&Slot<T>) + Send,
{
    fn call_box(self: Box<F>, other: &Slot<T>) {
        (*self)(other)
    }
}

trait FnBox2<T>: Send {
    fn call_box(self: Box<Self>, other: &Slot<T>, Option<T>);
}

impl<T, F> FnBox2<T> for F
    where F: FnOnce(&Slot<T>, Option<T>) + Send,
{
    fn call_box(self: Box<F>, other: &Slot<T>, item: Option<T>) {
        (*self)(other, item)
    }
}

impl State {
    fn flag(&self, f: usize) -> bool {
        self.0 & f != 0
    }

    fn set_flag(&self, f: usize, val: bool) -> State {
        State(if val {
            self.0 | f
        } else {
            self.0 & !f
        })
    }

    fn token(&self) -> usize {
        self.0 >> STATE_BITS
    }

    fn set_token(&self, gen: usize) -> State {
        State((gen << STATE_BITS) | (self.0 & STATE_MASK))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::thread;

    use super::Slot;

    #[test]
    fn sequential() {
        let slot = Slot::new(Some(1));

        // We can consume once
        assert_eq!(slot.try_consume(), Ok(1));
        assert!(slot.try_consume().is_err());

        // Consume a production
        assert_eq!(slot.try_produce(2), Ok(()));
        assert_eq!(slot.try_consume(), Ok(2));

        // Can't produce twice
        assert_eq!(slot.try_produce(3), Ok(()));
        assert!(slot.try_produce(3).is_err());

        // on_full is run immediately if full
        let hit = Arc::new(AtomicUsize::new(0));
        let hit2 = hit.clone();
        slot.on_full(move |_s| {
            hit2.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(hit.load(Ordering::SeqCst), 1);

        // on_full can be run twice, and we can consume in the callback
        let hit2 = hit.clone();
        slot.on_full(move |s| {
            hit2.fetch_add(1, Ordering::SeqCst);
            assert_eq!(s.try_consume(), Ok(3));
        });
        assert_eq!(hit.load(Ordering::SeqCst), 2);

        // Production can't run a previous callback
        assert_eq!(slot.try_produce(4), Ok(()));
        assert_eq!(hit.load(Ordering::SeqCst), 2);
        assert_eq!(slot.try_consume(), Ok(4));

        // Productions run new callbacks
        let hit2 = hit.clone();
        slot.on_full(move |s| {
            hit2.fetch_add(1, Ordering::SeqCst);
            assert_eq!(s.try_consume(), Ok(5));
        });
        assert_eq!(slot.try_produce(5), Ok(()));
        assert_eq!(hit.load(Ordering::SeqCst), 3);

        // on empty should fire immediately for an empty slot
        let hit2 = hit.clone();
        slot.on_empty(None, move |_, _| {
            hit2.fetch_add(1, Ordering::SeqCst);
        });
        assert_eq!(hit.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn channel() {
        const N: usize = 10000;

        struct Sender {
            slot: Arc<Slot<usize>>,
            hit: Arc<AtomicUsize>,
        }

        struct Receiver {
            slot: Arc<Slot<usize>>,
            hit: Arc<AtomicUsize>,
        }

        impl Sender {
            fn send(&self, val: usize) {
                if self.slot.try_produce(val).is_ok() {
                    return
                }
                let me = thread::current();
                self.hit.store(0, Ordering::SeqCst);
                let hit = self.hit.clone();
                self.slot.on_empty(None, move |_slot, _| {
                    hit.store(1, Ordering::SeqCst);
                    me.unpark();
                });
                while self.hit.load(Ordering::SeqCst) == 0 {
                    thread::park();
                }
                self.slot.try_produce(val).expect("can't produce after on_empty")
            }
        }

        impl Receiver {
            fn recv(&self) -> usize {
                if let Ok(i) = self.slot.try_consume() {
                    return i
                }

                let me = thread::current();
                self.hit.store(0, Ordering::SeqCst);
                let hit = self.hit.clone();
                self.slot.on_full(move |_slot| {
                    hit.store(1, Ordering::SeqCst);
                    me.unpark();
                });
                while self.hit.load(Ordering::SeqCst) == 0 {
                    thread::park();
                }
                self.slot.try_consume().expect("can't consume after on_full")
            }
        }

        let slot = Arc::new(Slot::new(None));
        let slot2 = slot.clone();

        let tx = Sender { slot: slot2, hit: Arc::new(AtomicUsize::new(0)) };
        let rx = Receiver { slot: slot, hit: Arc::new(AtomicUsize::new(0)) };

        let a = thread::spawn(move || {
            for i in 0..N {
                assert_eq!(rx.recv(), i);
            }
        });

        for i in 0..N {
            tx.send(i);
        }

        a.join().unwrap();
    }

    #[test]
    fn cancel() {
        let slot = Slot::new(None);
        let hits = Arc::new(AtomicUsize::new(0));

        let add = || {
            let hits = hits.clone();
            move |_: &Slot<u32>| { hits.fetch_add(1, Ordering::SeqCst); }
        };
        let add_empty = || {
            let hits = hits.clone();
            move |_: &Slot<u32>, _: Option<u32>| {
                hits.fetch_add(1, Ordering::SeqCst);
            }
        };

        // cancel on_full
        let n = hits.load(Ordering::SeqCst);
        assert_eq!(hits.load(Ordering::SeqCst), n);
        let token = slot.on_full(add());
        assert_eq!(hits.load(Ordering::SeqCst), n);
        slot.cancel(token);
        assert_eq!(hits.load(Ordering::SeqCst), n);
        assert!(slot.try_consume().is_err());
        assert!(slot.try_produce(1).is_ok());
        assert!(slot.try_consume().is_ok());
        assert_eq!(hits.load(Ordering::SeqCst), n);

        // cancel on_empty
        let n = hits.load(Ordering::SeqCst);
        assert_eq!(hits.load(Ordering::SeqCst), n);
        slot.try_produce(1).unwrap();
        let token = slot.on_empty(None, add_empty());
        assert_eq!(hits.load(Ordering::SeqCst), n);
        slot.cancel(token);
        assert_eq!(hits.load(Ordering::SeqCst), n);
        assert!(slot.try_produce(1).is_err());

        // cancel with no effect
        let n = hits.load(Ordering::SeqCst);
        assert_eq!(hits.load(Ordering::SeqCst), n);
        let token = slot.on_full(add());
        assert_eq!(hits.load(Ordering::SeqCst), n + 1);
        slot.cancel(token);
        assert_eq!(hits.load(Ordering::SeqCst), n + 1);
        assert!(slot.try_consume().is_ok());
        let token = slot.on_empty(None, add_empty());
        assert_eq!(hits.load(Ordering::SeqCst), n + 2);
        slot.cancel(token);
        assert_eq!(hits.load(Ordering::SeqCst), n + 2);

        // cancel old ones don't count
        let n = hits.load(Ordering::SeqCst);
        assert_eq!(hits.load(Ordering::SeqCst), n);
        let token1 = slot.on_full(add());
        assert_eq!(hits.load(Ordering::SeqCst), n);
        assert!(slot.try_produce(1).is_ok());
        assert_eq!(hits.load(Ordering::SeqCst), n + 1);
        assert!(slot.try_consume().is_ok());
        assert_eq!(hits.load(Ordering::SeqCst), n + 1);
        let token2 = slot.on_full(add());
        assert_eq!(hits.load(Ordering::SeqCst), n + 1);
        slot.cancel(token1);
        assert_eq!(hits.load(Ordering::SeqCst), n + 1);
        slot.cancel(token2);
        assert_eq!(hits.load(Ordering::SeqCst), n + 1);
    }
}
