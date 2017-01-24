//! The core reactor driving all I/O
//!
//! This module contains the `Core` type which is the reactor for all I/O
//! happening in `tokio-core`. This reactor (or event loop) is used to run
//! futures, schedule tasks, issue I/O requests, etc.

use std::cell::RefCell;
use std::cmp;
use std::io::{self, ErrorKind};
use std::mem;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::time::{Instant, Duration};

use futures::{self, Future, IntoFuture, Async};
use futures::executor::{self, Spawn, Unpark};
use futures::sync::mpsc;
use futures::task::Task;
use mio;
use slab::Slab;

use heap::{Heap, Slot};

mod io_token;
mod timeout_token;

mod poll_evented;
mod timeout;
mod interval;
pub use self::poll_evented::PollEvented;
pub use self::timeout::Timeout;
pub use self::interval::Interval;

static NEXT_LOOP_ID: AtomicUsize = ATOMIC_USIZE_INIT;
scoped_thread_local!(static CURRENT_LOOP: Core);

/// An event loop.
///
/// The event loop is the main source of blocking in an application which drives
/// all other I/O events and notifications happening. Each event loop can have
/// multiple handles pointing to it, each of which can then be used to create
/// various I/O objects to interact with the event loop in interesting ways.
// TODO: expand this
pub struct Core {
    events: mio::Events,
    tx: mpsc::UnboundedSender<Message>,
    rx: RefCell<Spawn<mpsc::UnboundedReceiver<Message>>>,
    _rx_registration: mio::Registration,
    rx_readiness: Arc<MySetReadiness>,

    inner: Rc<RefCell<Inner>>,

    // Used for determining when the future passed to `run` is ready. Once the
    // registration is passed to `io` above we never touch it again, just keep
    // it alive.
    _future_registration: mio::Registration,
    future_readiness: Arc<MySetReadiness>,
}

struct Inner {
    id: usize,
    io: mio::Poll,

    // Dispatch slabs for I/O and futures events
    io_dispatch: Slab<ScheduledIo>,
    task_dispatch: Slab<ScheduledTask>,

    // Timer wheel keeping track of all timeouts. The `usize` stored in the
    // timer wheel is an index into the slab below.
    //
    // The slab below keeps track of the timeouts themselves as well as the
    // state of the timeout itself. The `TimeoutToken` type is an index into the
    // `timeouts` slab.
    timer_heap: Heap<(Instant, usize)>,
    timeouts: Slab<(Option<Slot>, TimeoutState)>,
}

/// An unique ID for a Core
///
/// An ID by which different cores may be distinguished. Can be compared and used as an index in
/// a `HashMap`.
///
/// The ID is globally unique and never reused.
#[derive(Clone,Copy,Eq,PartialEq,Hash,Debug)]
pub struct CoreId(usize);

/// Handle to an event loop, used to construct I/O objects, send messages, and
/// otherwise interact indirectly with the event loop itself.
///
/// Handles can be cloned, and when cloned they will still refer to the
/// same underlying event loop.
#[derive(Clone)]
pub struct Remote {
    id: usize,
    tx: mpsc::UnboundedSender<Message>,
}

/// A non-sendable handle to an event loop, useful for manufacturing instances
/// of `LoopData`.
#[derive(Clone)]
pub struct Handle {
    remote: Remote,
    inner: Weak<RefCell<Inner>>,
}

struct ScheduledIo {
    readiness: Arc<AtomicUsize>,
    reader: Option<Task>,
    writer: Option<Task>,
}

struct ScheduledTask {
    _registration: mio::Registration,
    spawn: Option<Spawn<Box<Future<Item=(), Error=()>>>>,
    wake: Arc<MySetReadiness>,
}

enum TimeoutState {
    NotFired,
    Fired,
    Waiting(Task),
}

enum Direction {
    Read,
    Write,
}

enum Message {
    DropSource(usize),
    Schedule(usize, Task, Direction),
    UpdateTimeout(usize, Task),
    ResetTimeout(usize, Instant),
    CancelTimeout(usize),
    Run(Box<FnBox>),
}

const TOKEN_MESSAGES: mio::Token = mio::Token(0);
const TOKEN_FUTURE: mio::Token = mio::Token(1);
const TOKEN_START: usize = 2;

impl Core {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Core> {
        let io = try!(mio::Poll::new());
        let future_pair = mio::Registration::new(&io,
                                                 TOKEN_FUTURE,
                                                 mio::Ready::readable(),
                                                 mio::PollOpt::level());
        let (tx, rx) = mpsc::unbounded();
        let channel_pair = mio::Registration::new(&io,
                                                  TOKEN_MESSAGES,
                                                  mio::Ready::readable(),
                                                  mio::PollOpt::level());
        let rx_readiness = Arc::new(MySetReadiness(channel_pair.1));
        rx_readiness.unpark();

        Ok(Core {
            events: mio::Events::with_capacity(1024),
            tx: tx,
            rx: RefCell::new(executor::spawn(rx)),
            _rx_registration: channel_pair.0,
            rx_readiness: rx_readiness,

            _future_registration: future_pair.0,
            future_readiness: Arc::new(MySetReadiness(future_pair.1)),

            inner: Rc::new(RefCell::new(Inner {
                id: NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed),
                io: io,
                io_dispatch: Slab::with_capacity(1),
                task_dispatch: Slab::with_capacity(1),
                timeouts: Slab::with_capacity(1),
                timer_heap: Heap::new(),
            })),
        })
    }

    /// Returns a handle to this event loop which cannot be sent across threads
    /// but can be used as a proxy to the event loop itself.
    ///
    /// Handles are cloneable and clones always refer to the same event loop.
    /// This handle is typically passed into functions that create I/O objects
    /// to bind them to this event loop.
    pub fn handle(&self) -> Handle {
        Handle {
            remote: self.remote(),
            inner: Rc::downgrade(&self.inner),
        }
    }

    /// Generates a remote handle to this event loop which can be used to spawn
    /// tasks from other threads into this event loop.
    pub fn remote(&self) -> Remote {
        Remote {
            id: self.inner.borrow().id,
            tx: self.tx.clone(),
        }
    }

    /// Runs a future until completion, driving the event loop while we're
    /// otherwise waiting for the future to complete.
    ///
    /// This function will begin executing the event loop and will finish once
    /// the provided future is resolve. Note that the future argument here
    /// crucially does not require the `'static` nor `Send` bounds. As a result
    /// the future will be "pinned" to not only this thread but also this stack
    /// frame.
    ///
    /// This function will returns the value that the future resolves to once
    /// the future has finished. If the future never resolves then this function
    /// will never return.
    ///
    /// # Panics
    ///
    /// This method will **not** catch panics from polling the future `f`. If
    /// the future panics then it's the responsibility of the caller to catch
    /// that panic and handle it as appropriate.
    pub fn run<F>(&mut self, f: F) -> Result<F::Item, F::Error>
        where F: Future,
    {
        let mut task = executor::spawn(f);
        let ready = self.future_readiness.clone();
        let mut future_fired = true;

        loop {
            if future_fired {
                let res = try!(CURRENT_LOOP.set(self, || {
                    task.poll_future(ready.clone())
                }));
                if let Async::Ready(e) = res {
                    return Ok(e)
                }
            }
            future_fired = self.poll(None);
        }
    }

    /// Performs one iteration of the event loop, blocking on waiting for events
    /// for at most `max_wait` (forever if `None`).
    ///
    /// It only makes sense to call this method if you've previously spawned
    /// a future onto this event loop.
    ///
    /// `loop { lp.turn(None) }` is equivalent to calling `run` with an
    /// empty future (one that never finishes).
    pub fn turn(&mut self, max_wait: Option<Duration>) {
        self.poll(max_wait);
    }

    fn poll(&mut self, max_wait: Option<Duration>) -> bool {
        // Given the `max_wait` variable specified, figure out the actual
        // timeout that we're going to pass to `poll`. This involves taking a
        // look at active timers on our heap as well.
        let start = Instant::now();
        let timeout = self.inner.borrow_mut().timer_heap.peek().map(|t| {
            if t.0 < start {
                Duration::new(0, 0)
            } else {
                t.0 - start
            }
        });
        let timeout = match (max_wait, timeout) {
            (Some(d1), Some(d2)) => Some(cmp::min(d1, d2)),
            (max_wait, timeout) => max_wait.or(timeout),
        };

        // Block waiting for an event to happen, peeling out how many events
        // happened.
        let amt = match self.inner.borrow_mut().io.poll(&mut self.events, timeout) {
            Ok(a) => a,
            Err(ref e) if e.kind() == ErrorKind::Interrupted => return false,
            Err(e) => panic!("error in poll: {}", e),
        };

        let after_poll = Instant::now();
        debug!("loop poll - {:?}", after_poll - start);
        debug!("loop time - {:?}", after_poll);

        // Process all timeouts that may have just occurred, updating the
        // current time since
        self.consume_timeouts(after_poll);

        // Process all the events that came in, dispatching appropriately
        let mut fired = false;
        for i in 0..self.events.len() {
            let event = self.events.get(i).unwrap();
            let token = event.token();
            trace!("event {:?} {:?}", event.kind(), event.token());

            if token == TOKEN_MESSAGES {
                self.rx_readiness.0.set_readiness(mio::Ready::none()).unwrap();
                CURRENT_LOOP.set(&self, || self.consume_queue());
            } else if token == TOKEN_FUTURE {
                self.future_readiness.0.set_readiness(mio::Ready::none()).unwrap();
                fired = true;
            } else {
                self.dispatch(token, event.kind());
            }
        }
        debug!("loop process - {} events, {:?}", amt, after_poll.elapsed());
        return fired
    }

    fn dispatch(&mut self, token: mio::Token, ready: mio::Ready) {
        let token = usize::from(token) - TOKEN_START;
        if token % 2 == 0 {
            self.dispatch_io(token / 2, ready)
        } else {
            self.dispatch_task(token / 2)
        }
    }

    fn dispatch_io(&mut self, token: usize, ready: mio::Ready) {
        let mut reader = None;
        let mut writer = None;
        let mut inner = self.inner.borrow_mut();
        if let Some(io) = inner.io_dispatch.get_mut(token) {
            if ready.is_readable() || ready.is_hup() {
                reader = io.reader.take();
                io.readiness.fetch_or(1, Ordering::Relaxed);
            }
            if ready.is_writable() {
                writer = io.writer.take();
                io.readiness.fetch_or(2, Ordering::Relaxed);
            }
        }
        drop(inner);
        // TODO: don't notify the same task twice
        if let Some(reader) = reader {
            self.notify_handle(reader);
        }
        if let Some(writer) = writer {
            self.notify_handle(writer);
        }
    }

    fn dispatch_task(&mut self, token: usize) {
        let mut inner = self.inner.borrow_mut();
        let (task, wake) = match inner.task_dispatch.get_mut(token) {
            Some(slot) => (slot.spawn.take(), slot.wake.clone()),
            None => return,
        };
        wake.0.set_readiness(mio::Ready::none()).unwrap();
        let mut task = match task {
            Some(task) => task,
            None => return,
        };
        drop(inner);
        let res = CURRENT_LOOP.set(self, || task.poll_future(wake));
        inner = self.inner.borrow_mut();
        match res {
            Ok(Async::NotReady) => {
                assert!(inner.task_dispatch[token].spawn.is_none());
                inner.task_dispatch[token].spawn = Some(task);
            }
            Ok(Async::Ready(())) |
            Err(()) => {
                inner.task_dispatch.remove(token).unwrap();
            }
        }
    }

    fn consume_timeouts(&mut self, now: Instant) {
        loop {
            let mut inner = self.inner.borrow_mut();
            match inner.timer_heap.peek() {
                Some(head) if head.0 <= now => {}
                Some(_) => break,
                None => break,
            };
            let (_, slab_idx) = inner.timer_heap.pop().unwrap();

            trace!("firing timeout: {}", slab_idx);
            inner.timeouts[slab_idx].0.take().unwrap();
            let handle = inner.timeouts[slab_idx].1.fire();
            drop(inner);
            if let Some(handle) = handle {
                self.notify_handle(handle);
            }
        }
    }

    /// Method used to notify a task handle.
    ///
    /// Note that this should be used instead of `handle.unpark()` to ensure
    /// that the `CURRENT_LOOP` variable is set appropriately.
    fn notify_handle(&self, handle: Task) {
        debug!("notifying a task handle");
        CURRENT_LOOP.set(&self, || handle.unpark());
    }

    fn consume_queue(&self) {
        debug!("consuming notification queue");
        // TODO: can we do better than `.unwrap()` here?
        let unpark = self.rx_readiness.clone();
        loop {
            match self.rx.borrow_mut().poll_stream(unpark.clone()).unwrap() {
                Async::Ready(Some(msg)) => self.notify(msg),
                Async::NotReady |
                Async::Ready(None) => break,
            }
        }
    }

    fn notify(&self, msg: Message) {
        match msg {
            Message::DropSource(tok) => self.inner.borrow_mut().drop_source(tok),
            Message::Schedule(tok, wake, dir) => {
                let task = self.inner.borrow_mut().schedule(tok, wake, dir);
                if let Some(task) = task {
                    self.notify_handle(task);
                }
            }
            Message::UpdateTimeout(t, handle) => {
                let task = self.inner.borrow_mut().update_timeout(t, handle);
                if let Some(task) = task {
                    self.notify_handle(task);
                }
            }
            Message::ResetTimeout(t, at) => {
                self.inner.borrow_mut().reset_timeout(t, at);
            }
            Message::CancelTimeout(t) => {
                self.inner.borrow_mut().cancel_timeout(t)
            }
            Message::Run(r) => r.call_box(self),
        }
    }

    /// Get the ID of this loop
    pub fn id(&self) -> CoreId {
        CoreId(self.inner.borrow().id)
    }
}

impl Inner {
    fn add_source(&mut self, source: &mio::Evented)
                  -> io::Result<(Arc<AtomicUsize>, usize)> {
        debug!("adding a new I/O source");
        let sched = ScheduledIo {
            readiness: Arc::new(AtomicUsize::new(0)),
            reader: None,
            writer: None,
        };
        if self.io_dispatch.vacant_entry().is_none() {
            let amt = self.io_dispatch.len();
            self.io_dispatch.reserve_exact(amt);
        }
        let entry = self.io_dispatch.vacant_entry().unwrap();
        try!(self.io.register(source,
                              mio::Token(TOKEN_START + entry.index() * 2),
                              mio::Ready::readable() | mio::Ready::writable() | mio::Ready::hup(),
                              mio::PollOpt::edge()));
        Ok((sched.readiness.clone(), entry.insert(sched).index()))
    }

    fn deregister_source(&mut self, source: &mio::Evented) -> io::Result<()> {
        self.io.deregister(source)
    }

    fn drop_source(&mut self, token: usize) {
        debug!("dropping I/O source: {}", token);
        self.io_dispatch.remove(token).unwrap();
    }

    fn schedule(&mut self, token: usize, wake: Task, dir: Direction)
                -> Option<Task> {
        debug!("scheduling direction for: {}", token);
        let sched = self.io_dispatch.get_mut(token).unwrap();
        let (slot, bit) = match dir {
            Direction::Read => (&mut sched.reader, 1),
            Direction::Write => (&mut sched.writer, 2),
        };
        if sched.readiness.load(Ordering::SeqCst) & bit != 0 {
            *slot = None;
            Some(wake)
        } else {
            *slot = Some(wake);
            None
        }
    }

    fn add_timeout(&mut self, at: Instant) -> usize {
        if self.timeouts.vacant_entry().is_none() {
            let len = self.timeouts.len();
            self.timeouts.reserve_exact(len);
        }
        let entry = self.timeouts.vacant_entry().unwrap();
        let slot = self.timer_heap.push((at, entry.index()));
        let entry = entry.insert((Some(slot), TimeoutState::NotFired));
        debug!("added a timeout: {}", entry.index());
        return entry.index();
    }

    fn update_timeout(&mut self, token: usize, handle: Task) -> Option<Task> {
        debug!("updating a timeout: {}", token);
        self.timeouts[token].1.block(handle)
    }

    fn reset_timeout(&mut self, token: usize, at: Instant) {
        let pair = &mut self.timeouts[token];
        // TODO: avoid remove + push and instead just do one sift of the heap?
        // In theory we could update it in place and then do the percolation
        // as necessary
        if let Some(slot) = pair.0.take() {
            self.timer_heap.remove(slot);
        }
        let slot = self.timer_heap.push((at, token));
        *pair = (Some(slot), TimeoutState::NotFired);
        debug!("set a timeout: {}", token);
    }

    fn cancel_timeout(&mut self, token: usize) {
        debug!("cancel a timeout: {}", token);
        let pair = self.timeouts.remove(token);
        if let Some((Some(slot), _state)) = pair {
            self.timer_heap.remove(slot);
        }
    }

    fn spawn(&mut self, future: Box<Future<Item=(), Error=()>>) {
        if self.task_dispatch.vacant_entry().is_none() {
            let len = self.task_dispatch.len();
            self.task_dispatch.reserve_exact(len);
        }
        let entry = self.task_dispatch.vacant_entry().unwrap();
        let token = TOKEN_START + 2 * entry.index() + 1;
        let pair = mio::Registration::new(&self.io,
                                          mio::Token(token),
                                          mio::Ready::readable(),
                                          mio::PollOpt::level());
        let unpark = Arc::new(MySetReadiness(pair.1));
        let entry = entry.insert(ScheduledTask {
            spawn: Some(executor::spawn(future)),
            wake: unpark,
            _registration: pair.0,
        });
        entry.get().wake.clone().unpark();
    }
}

impl Remote {
    fn send(&self, msg: Message) {
        self.with_loop(|lp| {
            match lp {
                Some(lp) => {
                    // Need to execute all existing requests first, to ensure
                    // that our message is processed "in order"
                    lp.consume_queue();
                    lp.notify(msg);
                }
                None => {
                    match mpsc::UnboundedSender::send(&self.tx, msg) {
                        Ok(()) => {}

                        // TODO: this error should punt upwards and we should
                        //       notify the caller that the message wasn't
                        //       received. This is tokio-core#17
                        Err(e) => drop(e),
                    }
                }
            }
        })
    }

    fn with_loop<F, R>(&self, f: F) -> R
        where F: FnOnce(Option<&Core>) -> R
    {
        if CURRENT_LOOP.is_set() {
            CURRENT_LOOP.with(|lp| {
                let same = lp.inner.borrow().id == self.id;
                if same {
                    f(Some(lp))
                } else {
                    f(None)
                }
            })
        } else {
            f(None)
        }
    }

    /// Spawns a new future into the event loop this remote is associated with.
    ///
    /// This function takes a closure which is executed within the context of
    /// the I/O loop itself. The future returned by the closure will be
    /// scheduled on the event loop an run to completion.
    ///
    /// Note that while the closure, `F`, requires the `Send` bound as it might
    /// cross threads, the future `R` does not.
    pub fn spawn<F, R>(&self, f: F)
        where F: FnOnce(&Handle) -> R + Send + 'static,
              R: IntoFuture<Item=(), Error=()>,
              R::Future: 'static,
    {
        self.send(Message::Run(Box::new(|lp: &Core| {
            let f = f(&lp.handle());
            lp.inner.borrow_mut().spawn(Box::new(f.into_future()));
        })));
    }

    /// Return the ID of the represented Core
    pub fn id(&self) -> CoreId {
        CoreId(self.id)
    }

    /// Attempts to "promote" this remote to a handle, if possible.
    ///
    /// This function is intended for structures which typically work through a
    /// `Remote` but want to optimize runtime when the remote doesn't actually
    /// leave the thread of the original reactor. This will attempt to return a
    /// handle if the `Remote` is on the same thread as the event loop and the
    /// event loop is running.
    ///
    /// If this `Remote` has moved to a different thread or if the event loop is
    /// running, then `None` may be returned. If you need to guarantee access to
    /// a `Handle`, then you can call this function and fall back to using
    /// `spawn` above if it returns `None`.
    pub fn handle(&self) -> Option<Handle> {
        if CURRENT_LOOP.is_set() {
            CURRENT_LOOP.with(|lp| {
                let same = lp.inner.borrow().id == self.id;
                if same {
                    Some(lp.handle())
                } else {
                    None
                }
            })
        } else {
            None
        }
    }
}

impl Handle {
    /// Returns a reference to the underlying remote handle to the event loop.
    pub fn remote(&self) -> &Remote {
        &self.remote
    }

    /// Spawns a new future on the event loop this handle is associated with.
    pub fn spawn<F>(&self, f: F)
        where F: Future<Item=(), Error=()> + 'static,
    {
        let inner = match self.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };
        inner.borrow_mut().spawn(Box::new(f));
    }

    /// Spawns a closure on this event loop.
    ///
    /// This function is a convenience wrapper around the `spawn` function above
    /// for running a closure wrapped in `futures::lazy`. It will spawn the
    /// function `f` provided onto the event loop, and continue to run the
    /// future returned by `f` on the event loop as well.
    pub fn spawn_fn<F, R>(&self, f: F)
        where F: FnOnce() -> R + 'static,
              R: IntoFuture<Item=(), Error=()> + 'static,
    {
        self.spawn(futures::lazy(f))
    }

    /// Return the ID of the represented Core
    pub fn id(&self) -> CoreId {
        self.remote.id()
    }
}

impl TimeoutState {
    fn block(&mut self, handle: Task) -> Option<Task> {
        match *self {
            TimeoutState::Fired => return Some(handle),
            _ => {}
        }
        *self = TimeoutState::Waiting(handle);
        None
    }

    fn fire(&mut self) -> Option<Task> {
        match mem::replace(self, TimeoutState::Fired) {
            TimeoutState::NotFired => None,
            TimeoutState::Fired => panic!("fired twice?"),
            TimeoutState::Waiting(handle) => Some(handle),
        }
    }
}

struct MySetReadiness(mio::SetReadiness);

impl Unpark for MySetReadiness {
    fn unpark(&self) {
        self.0.set_readiness(mio::Ready::readable())
              .expect("failed to set readiness");
    }
}

trait FnBox: Send + 'static {
    fn call_box(self: Box<Self>, lp: &Core);
}

impl<F: FnOnce(&Core) + Send + 'static> FnBox for F {
    fn call_box(self: Box<Self>, lp: &Core) {
        (*self)(lp)
    }
}
