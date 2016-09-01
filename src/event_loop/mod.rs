use std::cell::RefCell;
use std::io::{self, ErrorKind};
use std::mem;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::time::{Instant, Duration};

use futures::{Future, Poll, IntoFuture, Async};
use futures::task::{self, Unpark, Task, Spawn};
use mio;
use slab::Slab;

use slot::{self, Slot};
use timer_wheel::{TimerWheel, Timeout};

mod channel;
mod source;
mod timeout;
pub use self::source::{AddSource, IoToken};
pub use self::timeout::{AddTimeout, TimeoutToken};
use self::channel::{Sender, Receiver, channel};

static NEXT_LOOP_ID: AtomicUsize = ATOMIC_USIZE_INIT;
scoped_thread_local!(static CURRENT_LOOP: Loop);

const SLAB_CAPACITY: usize = 1024 * 64;

/// An event loop.
///
/// The event loop is the main source of blocking in an application which drives
/// all other I/O events and notifications happening. Each event loop can have
/// multiple handles pointing to it, each of which can then be used to create
/// various I/O objects to interact with the event loop in interesting ways.
// TODO: expand this
pub struct Loop {
    id: usize,
    io: mio::Poll,
    events: mio::Events,
    tx: Arc<Sender<Message>>,
    rx: Receiver<Message>,
    io_dispatch: RefCell<Slab<ScheduledIo, usize>>,
    task_dispatch: RefCell<Slab<ScheduledTask, usize>>,

    // Incoming queue of newly spawned futures
    new_futures: Rc<NewFutures>,
    _new_futures_registration: mio::Registration,

    // Used for determining when the future passed to `run` is ready. Once the
    // registration is passed to `io` above we never touch it again, just keep
    // it alive.
    _future_registration: mio::Registration,
    future_readiness: Arc<MySetReadiness>,

    // Timer wheel keeping track of all timeouts. The `usize` stored in the
    // timer wheel is an index into the slab below.
    //
    // The slab below keeps track of the timeouts themselves as well as the
    // state of the timeout itself. The `TimeoutToken` type is an index into the
    // `timeouts` slab.
    timer_wheel: RefCell<TimerWheel<usize>>,
    timeouts: RefCell<Slab<(Timeout, TimeoutState), usize>>,
}

/// Handle to an event loop, used to construct I/O objects, send messages, and
/// otherwise interact indirectly with the event loop itself.
///
/// Handles can be cloned, and when cloned they will still refer to the
/// same underlying event loop.
#[derive(Clone)]
pub struct LoopHandle {
    id: usize,
    tx: Arc<Sender<Message>>,
}

/// A non-sendable handle to an event loop, useful for manufacturing instances
/// of `LoopData`.
#[derive(Clone)]
pub struct LoopPin {
    handle: LoopHandle,
    futures: Rc<NewFutures>,
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

struct NewFutures {
    queue: RefCell<Vec<Box<Future<Item=(), Error=()>>>>,
    ready: mio::SetReadiness,
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
    AddTimeout(Instant, Arc<Slot<io::Result<(usize, Instant)>>>),
    UpdateTimeout(usize, Task),
    CancelTimeout(usize),
    Run(Box<FnBox>),
}

const TOKEN_MESSAGES: mio::Token = mio::Token(0);
const TOKEN_FUTURE: mio::Token = mio::Token(1);
const TOKEN_NEW_FUTURES: mio::Token = mio::Token(2);
const TOKEN_START: usize = 3;

impl Loop {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Loop> {
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
        let new_future_pair = mio::Registration::new(&io,
                                                     TOKEN_NEW_FUTURES,
                                                     mio::Ready::readable(),
                                                     mio::PollOpt::level());
        Ok(Loop {
            id: NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed),
            io: io,
            events: mio::Events::with_capacity(1024),
            tx: Arc::new(tx),
            rx: rx,
            io_dispatch: RefCell::new(Slab::with_capacity(SLAB_CAPACITY)),
            task_dispatch: RefCell::new(Slab::with_capacity(SLAB_CAPACITY)),
            timeouts: RefCell::new(Slab::with_capacity(SLAB_CAPACITY)),
            timer_wheel: RefCell::new(TimerWheel::new()),
            _future_registration: future_pair.0,
            future_readiness: Arc::new(MySetReadiness(future_pair.1)),
            _new_futures_registration: new_future_pair.0,
            new_futures: Rc::new(NewFutures {
                queue: RefCell::new(Vec::new()),
                ready: new_future_pair.1,
            }),
        })
    }

    /// Generates a handle to this event loop used to construct I/O objects and
    /// send messages.
    ///
    /// Handles to an event loop are cloneable as well and clones will always
    /// refer to the same event loop.
    pub fn handle(&self) -> LoopHandle {
        LoopHandle {
            id: self.id,
            tx: self.tx.clone(),
        }
    }

    /// Returns a "pin" of this event loop which cannot be sent across threads
    /// but can be used as a proxy to the event loop itself.
    ///
    /// Currently the primary use for this is to use as a handle to add data
    /// to the event loop directly. The `LoopPin::add_loop_data` method can
    /// be used to immediately create instances of `LoopData` structures.
    pub fn pin(&self) -> LoopPin {
        LoopPin {
            handle: self.handle(),
            futures: self.new_futures.clone(),
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
                let timeout = self.timer_wheel.borrow().next_timeout().map(|t| {
                    if t < start {
                        Duration::new(0, 0)
                    } else {
                        t - start
                    }
                });
                match self.io.poll(&mut self.events, timeout) {
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
                } else if token == TOKEN_NEW_FUTURES {
                    self.new_futures.ready.set_readiness(mio::Ready::none()).unwrap();
                    let mut new_futures = self.new_futures.queue.borrow_mut();
                    for future in new_futures.drain(..) {
                        self.spawn(future);
                    }
                } else {
                    self.dispatch(token, event.kind());
                }
            }

            debug!("loop process - {} events, {:?}", amt, start.elapsed());
        }
    }

    fn dispatch(&self, token: mio::Token, ready: mio::Ready) {
        let token = usize::from(token) - TOKEN_START;
        if token % 2 == 0 {
            self.dispatch_io(token / 2, ready)
        } else {
            self.dispatch_task(token / 2)
        }
    }

    fn dispatch_io(&self, token: usize, ready: mio::Ready) {
        let mut reader = None;
        let mut writer = None;
        if let Some(io) = self.io_dispatch.borrow_mut().get_mut(token) {
            if ready.is_readable() {
                reader = io.reader.take();
                io.readiness.fetch_or(1, Ordering::Relaxed);
            }
            if ready.is_writable() {
                writer = io.writer.take();
                io.readiness.fetch_or(2, Ordering::Relaxed);
            }
        }
        // TODO: don't notify the same task twice
        if let Some(reader) = reader {
            self.notify_handle(reader);
        }
        if let Some(writer) = writer {
            self.notify_handle(writer);
        }
    }

    fn dispatch_task(&self, token: usize) {
        let (task, wake) = match self.task_dispatch.borrow_mut().get_mut(token) {
            Some(slot) => (slot.spawn.take(), slot.wake.clone()),
            None => return,
        };
        wake.0.set_readiness(mio::Ready::none()).unwrap();
        let mut task = match task {
            Some(task) => task,
            None => return,
        };
        let res = CURRENT_LOOP.set(self, || task.poll_future(wake));
        let mut dispatch = self.task_dispatch.borrow_mut();
        match res {
            Ok(Async::NotReady) => {
                assert!(dispatch[token].spawn.is_none());
                dispatch[token].spawn = Some(task);
            }
            Ok(Async::Ready(())) |
            Err(()) => {
                dispatch.remove(token).unwrap();
            }
        }
    }

    fn consume_timeouts(&mut self, now: Instant) {
        loop {
            let idx = match self.timer_wheel.borrow_mut().poll(now) {
                Some(idx) => idx,
                None => break,
            };
            trace!("firing timeout: {}", idx);
            let handle = self.timeouts.borrow_mut()[idx].1.fire();
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

    fn add_source(&self, source: &mio::Evented)
                  -> io::Result<(Arc<AtomicUsize>, usize)> {
        debug!("adding a new I/O source");
        let sched = ScheduledIo {
            readiness: Arc::new(AtomicUsize::new(0)),
            reader: None,
            writer: None,
        };
        let mut dispatch = self.io_dispatch.borrow_mut();
        if dispatch.vacant_entry().is_none() {
            let amt = dispatch.len();
            dispatch.reserve_exact(amt);
        }
        let entry = dispatch.vacant_entry().unwrap();
        try!(self.io.register(source,
                              mio::Token(TOKEN_START + entry.index() * 2),
                              mio::Ready::readable() | mio::Ready::writable(),
                              mio::PollOpt::edge()));
        Ok((sched.readiness.clone(), entry.insert(sched).index()))
    }

    fn drop_source(&self, token: usize) {
        debug!("dropping I/O source: {}", token);
        self.io_dispatch.borrow_mut().remove(token).unwrap();
    }

    fn schedule(&self, token: usize, wake: Task, dir: Direction) {
        debug!("scheduling direction for: {}", token);
        let to_call = {
            let mut dispatch = self.io_dispatch.borrow_mut();
            let sched = dispatch.get_mut(token).unwrap();
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
        };
        if let Some(to_call) = to_call {
            debug!("schedule immediately done");
            self.notify_handle(to_call);
        }
    }

    fn add_timeout(&self, at: Instant) -> io::Result<(usize, Instant)> {
        let mut timeouts = self.timeouts.borrow_mut();
        if timeouts.vacant_entry().is_none() {
            let len = timeouts.len();
            timeouts.reserve_exact(len);
        }
        let entry = timeouts.vacant_entry().unwrap();
        let timeout = self.timer_wheel.borrow_mut().insert(at, entry.index());
        let when = *timeout.when();
        let entry = entry.insert((timeout, TimeoutState::NotFired));
        debug!("added a timeout: {}", entry.index());
        Ok((entry.index(), when))
    }

    fn update_timeout(&self, token: usize, handle: Task) {
        debug!("updating a timeout: {}", token);
        let to_wake = self.timeouts.borrow_mut()[token].1.block(handle);
        if let Some(to_wake) = to_wake {
            self.notify_handle(to_wake);
        }
    }

    fn cancel_timeout(&self, token: usize) {
        debug!("cancel a timeout: {}", token);
        let pair = self.timeouts.borrow_mut().remove(token);
        if let Some((timeout, _state)) = pair {
            self.timer_wheel.borrow_mut().cancel(&timeout);
        }
    }

    fn spawn(&self, future: Box<Future<Item=(), Error=()>>) {
        let unpark = {
            let mut dispatch = self.task_dispatch.borrow_mut();
            if dispatch.vacant_entry().is_none() {
                let len = dispatch.len();
                dispatch.reserve_exact(len);
            }
            let entry = dispatch.vacant_entry().unwrap();
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
            entry.get().wake.clone()
        };
        unpark.unpark();
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
            Message::DropSource(tok) => self.drop_source(tok),
            Message::Schedule(tok, wake, dir) => self.schedule(tok, wake, dir),

            Message::AddTimeout(at, slot) => {
                slot.try_produce(self.add_timeout(at))
                    .ok().expect("interference with try_produce on timeout");
            }
            Message::UpdateTimeout(t, handle) => self.update_timeout(t, handle),
            Message::CancelTimeout(t) => self.cancel_timeout(t),
            Message::Run(r) => r.call_box(self),
        }
    }
}

impl LoopHandle {
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
        where F: FnOnce(Option<&Loop>) -> R
    {
        if CURRENT_LOOP.is_set() {
            CURRENT_LOOP.with(|lp| {
                if lp.id == self.id {
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
        where F: FnOnce(&LoopPin) -> R + Send + 'static,
              R: IntoFuture<Item=(), Error=()>,
              R::Future: 'static,
    {
        self.send(Message::Run(Box::new(|lp: &Loop| {
            let f = f(&lp.pin());
            lp.spawn(Box::new(f.into_future()));
        })));
    }
}

impl LoopPin {
    /// Returns a reference to the underlying handle to the event loop.
    pub fn handle(&self) -> &LoopHandle {
        &self.handle
    }

    /// Spawns a new future on the event loop this pin is associated this.
    pub fn spawn<F>(&self, f: F)
        where F: Future<Item=(), Error=()> + 'static,
    {
        self.futures.queue.borrow_mut().push(Box::new(f));
        self.futures.ready.set_readiness(mio::Ready::readable()).unwrap();
    }
}

struct LoopFuture<T, U> {
    loop_handle: LoopHandle,
    data: Option<U>,
    result: Option<(Arc<Slot<io::Result<T>>>, slot::Token)>,
}

impl<T, U> LoopFuture<T, U>
    where T: 'static,
{
    fn poll<F, G>(&mut self, f: F, g: G) -> Poll<T, io::Error>
        where F: FnOnce(&Loop, U) -> io::Result<T>,
              G: FnOnce(U, Arc<Slot<io::Result<T>>>) -> Message,
    {
        match self.result {
            Some((ref result, ref mut token)) => {
                result.cancel(*token);
                match result.try_consume() {
                    Ok(Ok(t)) => return Ok(t.into()),
                    Ok(Err(e)) => return Err(e),
                    Err(_) => {}
                }
                let task = task::park();
                *token = result.on_full(move |_| {
                    task.unpark();
                });
                Ok(Async::NotReady)
            }
            None => {
                let data = &mut self.data;
                let ret = self.loop_handle.with_loop(|lp| {
                    lp.map(|lp| f(lp, data.take().unwrap()))
                });
                if let Some(ret) = ret {
                    debug!("loop future done immediately on event loop");
                    return ret.map(|e| e.into())
                }
                debug!("loop future needs to send info to event loop");

                let task = task::park();
                let result = Arc::new(Slot::new(None));
                let token = result.on_full(move |_| {
                    task.unpark();
                });
                self.result = Some((result.clone(), token));
                self.loop_handle.send(g(data.take().unwrap(), result));
                Ok(Async::NotReady)
            }
        }
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
    fn call_box(self: Box<Self>, lp: &Loop);
}

impl<F: FnOnce(&Loop) + Send + 'static> FnBox for F {
    fn call_box(self: Box<Self>, lp: &Loop) {
        (*self)(lp)
    }
}
