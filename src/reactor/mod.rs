//! The core reactor driving all I/O
//!
//! This module contains the `Core` type which is the reactor for all I/O
//! happening in `tokio-core`. This reactor (or event loop) is used to run
//! futures, schedule tasks, issue I/O requests, etc.

use std::cell::RefCell;
use std::io::{self, ErrorKind};
use std::mem;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::time::{Instant, Duration};

use futures::{Future, IntoFuture, Async};
use futures::task::{self, Unpark, Task, Spawn};
use mio;
use slab::Slab;

use heap::{Heap, Slot};

mod channel;
mod io_token;
mod timeout_token;
use self::channel::{Sender, Receiver, channel};

mod poll_evented;
mod timeout;
pub use self::poll_evented::PollEvented;
pub use self::timeout::Timeout;

static NEXT_LOOP_ID: AtomicUsize = ATOMIC_USIZE_INIT;
scoped_thread_local!(static CURRENT_LOOP: Core);

const SLAB_CAPACITY: usize = 1024 * 64;

/// An event loop.
///
/// The event loop is the main source of blocking in an application which drives
/// all other I/O events and notifications happening. Each event loop can have
/// multiple handles pointing to it, each of which can then be used to create
/// various I/O objects to interact with the event loop in interesting ways.
// TODO: expand this
pub struct Core {
    events: mio::Events,
    tx: Sender<Message>,
    rx: Receiver<Message>,
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
    timeouts: Slab<(Slot, TimeoutState)>,
}

/// Handle to an event loop, used to construct I/O objects, send messages, and
/// otherwise interact indirectly with the event loop itself.
///
/// Handles can be cloned, and when cloned they will still refer to the
/// same underlying event loop.
#[derive(Clone)]
pub struct Remote {
    id: usize,
    tx: Sender<Message>,
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
        let (tx, rx) = channel();
        let io = try!(mio::Poll::new());
        try!(io.register(&rx,
                         TOKEN_MESSAGES,
                         mio::Ready::readable(),
                         mio::PollOpt::edge()));
        let future_pair = mio::Registration::new(&io,
                                                 TOKEN_FUTURE,
                                                 mio::Ready::readable(),
                                                 mio::PollOpt::level());
        Ok(Core {
            events: mio::Events::with_capacity(1024),
            tx: tx,
            rx: rx,
            _future_registration: future_pair.0,
            future_readiness: Arc::new(MySetReadiness(future_pair.1)),

            inner: Rc::new(RefCell::new(Inner {
                id: NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed),
                io: io,
                io_dispatch: Slab::with_capacity(SLAB_CAPACITY),
                task_dispatch: Slab::with_capacity(SLAB_CAPACITY),
                timeouts: Slab::with_capacity(SLAB_CAPACITY),
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
    ///
    /// Similarly, because the provided future will be pinned not only to this
    /// thread but also to this task, any attempt to poll the future on a
    /// separate thread will result in a panic. That is, calls to
    /// `task::poll_on` must be avoided.
    pub fn run<F>(&mut self, f: F) -> Result<F::Item, F::Error>
        where F: Future,
    {
        let mut task = task::spawn(f);
        let ready = self.future_readiness.clone();

        // Next, move all that data into a dynamically dispatched closure to cut
        // down on monomorphization costs. Inside this closure we unset the
        // readiness of the future (as we're about to poll it) and then we check
        // to see if it's done. If it's not then the event loop will turn again.
        let mut res = None;
        self._run(&mut || {
            assert!(res.is_none());
            match task.poll_future(ready.clone()) {
                Ok(Async::NotReady) => {}
                Ok(Async::Ready(e)) => res = Some(Ok(e)),
                Err(e) => res = Some(Err(e)),
            }
            res.is_some()
        });
        res.expect("run should not return until future is done")
    }

    fn _run(&mut self, done: &mut FnMut() -> bool) {
        // Check to see if we're done immediately, if so we shouldn't do any
        // work.
        if CURRENT_LOOP.set(self, || done()) {
            return
        }

        let mut finished = false;
        while !finished {
            let amt;
            // On Linux, Poll::poll is epoll_wait, which may return EINTR if a
            // ptracer attaches. This retry loop prevents crashing when
            // attaching strace, or similar.
            let start = Instant::now();
            loop {
                let inner = self.inner.borrow_mut();
                let timeout = inner.timer_heap.peek().map(|t| {
                    if t.0 < start {
                        Duration::new(0, 0)
                    } else {
                        t.0 - start
                    }
                });
                match inner.io.poll(&mut self.events, timeout) {
                    Ok(a) => {
                        amt = a;
                        break;
                    }
                    Err(ref e) if e.kind() == ErrorKind::Interrupted => {}
                    err @ Err(_) => {
                        err.unwrap();
                    }
                }
            }
            debug!("loop poll - {:?}", start.elapsed());
            debug!("loop time - {:?}", Instant::now());

            // First up, process all timeouts that may have just occurred.
            let start = Instant::now();
            self.consume_timeouts(start);

            // Next, process all the events that came in.
            for i in 0..self.events.len() {
                let event = self.events.get(i).unwrap();
                let token = event.token();
                trace!("event {:?} {:?}", event.kind(), event.token());

                if token == TOKEN_MESSAGES {
                    CURRENT_LOOP.set(&self, || self.consume_queue());
                } else if token == TOKEN_FUTURE {
                    self.future_readiness.0.set_readiness(mio::Ready::none()).unwrap();
                    if !finished && CURRENT_LOOP.set(self, || done()) {
                        finished = true;
                    }
                } else {
                    self.dispatch(token, event.kind());
                }
            }

            debug!("loop process - {} events, {:?}", amt, start.elapsed());
        }
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
            if ready.is_readable() {
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
            let handle = inner.timeouts[slab_idx].1.fire();
            drop(inner);
            if let Some(handle) = handle {
                self.notify_handle(handle);
            }
        }
    }

    /// Method used to notify a task handle.
    ///
    /// Note that this should be used instead fo `handle.unpark()` to ensure
    /// that the `CURRENT_LOOP` variable is set appropriately.
    fn notify_handle(&self, handle: Task) {
        debug!("notifying a task handle");
        CURRENT_LOOP.set(&self, || handle.unpark());
    }

    fn consume_queue(&self) {
        debug!("consuming notification queue");
        // TODO: can we do better than `.unwrap()` here?
        while let Some(msg) = self.rx.recv().unwrap() {
            self.notify(msg);
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
            Message::CancelTimeout(t) => {
                self.inner.borrow_mut().cancel_timeout(t)
            }
            Message::Run(r) => r.call_box(self),
        }
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
                              mio::Ready::readable() | mio::Ready::writable(),
                              mio::PollOpt::edge()));
        Ok((sched.readiness.clone(), entry.insert(sched).index()))
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

    fn add_timeout(&mut self, at: Instant) -> io::Result<(usize, Instant)> {
        if self.timeouts.vacant_entry().is_none() {
            let len = self.timeouts.len();
            self.timeouts.reserve_exact(len);
        }
        let entry = self.timeouts.vacant_entry().unwrap();
        let slot = self.timer_heap.push((at, entry.index()));
        let entry = entry.insert((slot, TimeoutState::NotFired));
        debug!("added a timeout: {}", entry.index());
        Ok((entry.index(), at))
    }

    fn update_timeout(&mut self, token: usize, handle: Task) -> Option<Task> {
        debug!("updating a timeout: {}", token);
        self.timeouts[token].1.block(handle)
    }

    fn cancel_timeout(&mut self, token: usize) {
        debug!("cancel a timeout: {}", token);
        let pair = self.timeouts.remove(token);
        if let Some((slot, _state)) = pair {
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
            spawn: Some(task::spawn(future)),
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
                    match self.tx.send(msg) {
                        Ok(()) => {}

                        // This should only happen when there was an error
                        // writing to the pipe to wake up the event loop,
                        // hopefully that never happens
                        Err(e) => {
                            panic!("error sending message to event loop: {}", e)
                        }
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

    /// Spawns a new future into the event loop this handle is associated this.
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
}

impl Handle {
    /// Returns a reference to the underlying remote handle to the event loop.
    pub fn remote(&self) -> &Remote {
        &self.remote
    }

    /// Spawns a new future on the event loop this pin is associated this.
    pub fn spawn<F>(&self, f: F)
        where F: Future<Item=(), Error=()> + 'static,
    {
        let inner = match self.inner.upgrade() {
            Some(inner) => inner,
            None => return,
        };
        inner.borrow_mut().spawn(Box::new(f));
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
