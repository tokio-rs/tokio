use std::cell::{Cell, RefCell};
use std::io::{self, ErrorKind};
use std::mem;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, ATOMIC_USIZE_INIT, Ordering};
use std::sync::mpsc;
use std::time::{Instant, Duration};

use futures::{Future, Task, TaskHandle, Poll};
use futures_io::Ready;
use mio::channel::SendError;
use mio;
use slab::Slab;

use slot::{self, Slot};
use timer_wheel::{TimerWheel, Timeout};

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
    active: Cell<bool>,
    io: mio::Poll,
    tx: mio::channel::Sender<Message>,
    rx: mio::channel::Receiver<Message>,
    dispatch: RefCell<Slab<Scheduled, usize>>,

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
    tx: mio::channel::Sender<Message>,
}

struct Scheduled {
    source: IoSource,
    waiter: Option<TaskHandle>,
}

enum TimeoutState {
    NotFired,
    Fired,
    Waiting(TaskHandle),
}

enum Message {
    AddSource(IoSource, Arc<Slot<io::Result<usize>>>),
    DropSource(usize),
    Schedule(usize, TaskHandle),
    Deschedule(usize),
    AddTimeout(Instant, Arc<Slot<io::Result<TimeoutToken>>>),
    UpdateTimeout(TimeoutToken, TaskHandle),
    CancelTimeout(TimeoutToken),
    Shutdown,
}

pub struct Source<E: ?Sized> {
    readiness: AtomicUsize,
    io: E,
}

pub type IoSource = Arc<Source<mio::Evented + Sync + Send>>;

fn register(poll: &mio::Poll,
            token: usize,
            sched: &Scheduled) -> io::Result<()> {
    poll.register(&sched.source.io,
                  mio::Token(token),
                  mio::EventSet::readable() | mio::EventSet::writable(),
                  mio::PollOpt::edge())
}

fn deregister(poll: &mio::Poll, sched: &Scheduled) {
    // TODO: handle error
    poll.deregister(&sched.source.io).unwrap();
}

impl Loop {
    /// Creates a new event loop, returning any error that happened during the
    /// creation.
    pub fn new() -> io::Result<Loop> {
        let (tx, rx) = mio::channel::from_std_channel(mpsc::channel());
        let io = try!(mio::Poll::new());
        try!(io.register(&rx,
                         mio::Token(0),
                         mio::EventSet::readable(),
                         mio::PollOpt::edge()));
        Ok(Loop {
            id: NEXT_LOOP_ID.fetch_add(1, Ordering::Relaxed),
            active: Cell::new(true),
            io: io,
            tx: tx,
            rx: rx,
            dispatch: RefCell::new(Slab::new_starting_at(1, SLAB_CAPACITY)),
            timeouts: RefCell::new(Slab::new_starting_at(0, SLAB_CAPACITY)),
            timer_wheel: RefCell::new(TimerWheel::new()),
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

    /// Runs a future until completion, driving the event loop while we're
    /// otherwise waiting for the future to complete.
    ///
    /// Returns the value that the future resolves to.
    pub fn run<F: Future>(&mut self, f: F) -> Result<F::Item, F::Error> {
        let (tx_res, rx_res) = mpsc::channel();
        let handle = self.handle();
        f.then(move |res| {
            handle.shutdown();
            tx_res.send(res)
        }).forget();

        self._run();

        rx_res.recv().unwrap()
    }

    fn _run(&mut self) {
        let mut events = mio::Events::new();
        self.active.set(true);
        while self.active.get() {
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
                match self.io.poll(&mut events, timeout) {
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

            // First up, process all timeouts that may have just occurred.
            let start = Instant::now();
            self.consume_timeouts(start);

            // Next, process all the events that came in.
            for i in 0..events.len() {
                let event = events.get(i).unwrap();
                let token = usize::from(event.token());

                // Token 0 == our incoming message queue, so this means we
                // process the whole queue of messages.
                if token == 0 {
                    debug!("consuming notification queue");
                    self.consume_queue();
                    continue
                }

                // For any other token we look at `dispatch` to see what we're
                // supposed to do. If there's a waiter we get ready to notify
                // it, and we also or-in atomically any events that have
                // happened (currently read/write events).
                let mut waiter = None;
                if let Some(sched) = self.dispatch.get_mut().get_mut(token) {
                    waiter = sched.waiter.take();
                    if event.kind().is_readable() {
                        sched.source.readiness.fetch_or(1, Ordering::Relaxed);
                    }
                    if event.kind().is_writable() {
                        sched.source.readiness.fetch_or(2, Ordering::Relaxed);
                    }
                } else {
                    debug!("notified on {} which no longer exists", token);
                }

                // If we actually got a waiter, then notify!
                if let Some(waiter) = waiter {
                    self.notify_handle(waiter);
                }
            }

            debug!("loop process - {} events, {:?}", amt, start.elapsed());
        }

        debug!("loop is done!");
    }

    fn consume_timeouts(&mut self, now: Instant) {
        while let Some(idx) = self.timer_wheel.get_mut().poll(now) {
            trace!("firing timeout: {}", idx);
            let handle = self.timeouts.get_mut()[idx].1.fire();
            if let Some(handle) = handle {
                self.notify_handle(handle);
            }
        }
    }

    /// Method used to notify a task handle.
    ///
    /// Note that this should be used instead fo `handle.notify()` to ensure
    /// that the `CURRENT_LOOP` variable is set appropriately.
    fn notify_handle(&self, handle: TaskHandle) {
        CURRENT_LOOP.set(&self, || handle.notify());
    }

    fn add_source(&self, source: IoSource) -> io::Result<usize> {
        let sched = Scheduled {
            source: source,
            waiter: None,
        };
        let mut dispatch = self.dispatch.borrow_mut();
        if dispatch.vacant_entry().is_none() {
            let amt = dispatch.count();
            dispatch.grow(amt);
        }
        let entry = dispatch.vacant_entry().unwrap();
        try!(register(&self.io, entry.index(), &sched));
        Ok(entry.insert(sched).index())
    }

    fn drop_source(&self, token: usize) {
        let sched = self.dispatch.borrow_mut().remove(token).unwrap();
        deregister(&self.io, &sched);
    }

    fn schedule(&self, token: usize, wake: TaskHandle) {
        let to_call = {
            let mut dispatch = self.dispatch.borrow_mut();
            let sched = dispatch.get_mut(token).unwrap();
            if sched.source.readiness.load(Ordering::Relaxed) != 0 {
                sched.waiter = None;
                Some(wake)
            } else {
                sched.waiter = Some(wake);
                None
            }
        };
        if let Some(to_call) = to_call {
            self.notify_handle(to_call);
        }
    }

    fn deschedule(&self, token: usize) {
        let mut dispatch = self.dispatch.borrow_mut();
        let sched = dispatch.get_mut(token).unwrap();
        sched.waiter = None;
    }

    fn add_timeout(&self, at: Instant) -> io::Result<TimeoutToken> {
        let mut timeouts = self.timeouts.borrow_mut();
        if timeouts.vacant_entry().is_none() {
            let len = timeouts.count();
            timeouts.grow(len);
        }
        let entry = timeouts.vacant_entry().unwrap();
        let timeout = self.timer_wheel.borrow_mut().insert(at, entry.index());
        let entry = entry.insert((timeout, TimeoutState::NotFired));
        Ok(TimeoutToken { token: entry.index() })
    }

    fn update_timeout(&self, token: &TimeoutToken, handle: TaskHandle) {
        let to_wake = self.timeouts.borrow_mut()[token.token].1.block(handle);
        if let Some(to_wake) = to_wake {
            self.notify_handle(to_wake);
        }
    }

    fn cancel_timeout(&self, token: &TimeoutToken) {
        let pair = self.timeouts.borrow_mut().remove(token.token);
        if let Some((timeout, _state)) = pair {
            self.timer_wheel.borrow_mut().cancel(&timeout);
        }
    }

    fn consume_queue(&self) {
        while let Ok(msg) = self.rx.try_recv() {
            self.notify(msg);
        }
    }

    fn notify(&self, msg: Message) {
        match msg {
            Message::AddSource(source, slot) => {
                // This unwrap() should always be ok as we're the only producer
                slot.try_produce(self.add_source(source))
                    .ok().expect("interference with try_produce");
            }
            Message::DropSource(tok) => self.drop_source(tok),
            Message::Schedule(tok, wake) => self.schedule(tok, wake),
            Message::Deschedule(tok) => self.deschedule(tok),
            Message::Shutdown => self.active.set(false),

            Message::AddTimeout(at, slot) => {
                slot.try_produce(self.add_timeout(at))
                    .ok().expect("interference with try_produce on timeout");
            }
            Message::UpdateTimeout(t, handle) => self.update_timeout(&t, handle),
            Message::CancelTimeout(t) => self.cancel_timeout(&t),
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
                        Err(SendError::Io(e)) => {
                            panic!("error sending message to event loop: {}", e)
                        }

                        // If we're still sending a message to the event loop
                        // after it's closed, then that's bad!
                        Err(SendError::Disconnected(_)) => {
                            panic!("event loop is no longer available")
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

    /// Add a new source to an event loop, returning a future which will resolve
    /// to the token that can be used to identify this source.
    ///
    /// When a new I/O object is created it needs to be communicated to the
    /// event loop to ensure that it's registered and ready to receive
    /// notifications. The event loop with then respond with a unique token that
    /// this handle can be identified with (the resolved value of the returned
    /// future).
    ///
    /// This token is then passed in turn to each of the methods below to
    /// interact with notifications on the I/O object itself.
    ///
    /// # Panics
    ///
    /// The returned future will panic if the event loop this handle is
    /// associated with has gone away, or if there is an error communicating
    /// with the event loop.
    pub fn add_source(&self, source: IoSource) -> AddSource {
        AddSource {
            inner: LoopFuture {
                loop_handle: self.clone(),
                data: Some(source),
                result: None,
            }
        }
    }

    /// Begin listening for events on an event loop.
    ///
    /// Once an I/O object has been registered with the event loop through the
    /// `add_source` method, this method can be used with the assigned token to
    /// begin awaiting notifications.
    ///
    /// The `dir` argument indicates how the I/O object is expected to be
    /// awaited on (either readable or writable) and the `wake` callback will be
    /// invoked. Note that one the `wake` callback is invoked once it will not
    /// be invoked again, it must be re-`schedule`d to continue receiving
    /// notifications.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    pub fn schedule(&self, tok: usize, task: &mut Task) {
        // TODO: plumb through `&mut Task` if we're on the event loop
        self.send(Message::Schedule(tok, task.handle().clone()));
    }

    /// Stop listening for events on an event loop.
    ///
    /// Once a callback has been scheduled with the `schedule` method, it can be
    /// unregistered from the event loop with this method. This method does not
    /// guarantee that the callback will not be invoked if it hasn't already,
    /// but a best effort will be made to ensure it is not called.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    pub fn deschedule(&self, tok: usize) {
        self.send(Message::Deschedule(tok));
    }

    /// Unregister all information associated with a token on an event loop,
    /// deallocating all internal resources assigned to the given token.
    ///
    /// This method should be called whenever a source of events is being
    /// destroyed. This will ensure that the event loop can reuse `tok` for
    /// another I/O object if necessary and also remove it from any poll
    /// notifications and callbacks.
    ///
    /// Note that wake callbacks may still be invoked after this method is
    /// called as it may take some time for the message to drop a source to
    /// reach the event loop. Despite this fact, this method will attempt to
    /// ensure that the callbacks are **not** invoked, so pending scheduled
    /// callbacks cannot be relied upon to get called.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    pub fn drop_source(&self, tok: usize) {
        self.send(Message::DropSource(tok));
    }

    /// Adds a new timeout to get fired at the specified instant, notifying the
    /// specified task.
    pub fn add_timeout(&self, at: Instant) -> AddTimeout {
        AddTimeout {
            inner: LoopFuture {
                loop_handle: self.clone(),
                data: Some(at),
                result: None,
            },
        }
    }

    /// Updates a previously added timeout to notify a new task instead.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn update_timeout(&self, timeout: &TimeoutToken, task: &mut Task) {
        let timeout = TimeoutToken { token: timeout.token };
        self.send(Message::UpdateTimeout(timeout, task.handle().clone()))
    }

    /// Cancel a previously added timeout.
    ///
    /// # Panics
    ///
    /// This method will panic if the timeout specified was not created by this
    /// loop handle's `add_timeout` method.
    pub fn cancel_timeout(&self, timeout: &TimeoutToken) {
        let timeout = TimeoutToken { token: timeout.token };
        self.send(Message::CancelTimeout(timeout))
    }

    /// Send a message to the associated event loop that it should shut down, or
    /// otherwise break out of its current loop of iteration.
    ///
    /// This method does not forcibly cause the event loop to shut down or
    /// perform an interrupt on whatever task is currently running, instead a
    /// message is simply enqueued to at a later date process the request to
    /// stop looping ASAP.
    ///
    /// # Panics
    ///
    /// This function will panic if the event loop this handle is associated
    /// with has gone away, or if there is an error communicating with the event
    /// loop.
    pub fn shutdown(&self) {
        self.send(Message::Shutdown);
    }
}

/// A future which will resolve a unique `tok` token for an I/O object.
///
/// Created through the `LoopHandle::add_source` method, this future can also
/// resolve to an error if there's an issue communicating with the event loop.
pub struct AddSource {
    inner: LoopFuture<usize, IoSource>,
}

impl Future for AddSource {
    type Item = usize;
    type Error = io::Error;

    fn poll(&mut self, _task: &mut Task) -> Poll<usize, io::Error> {
        self.inner.poll(Loop::add_source)
    }

    fn schedule(&mut self, task: &mut Task) {
        self.inner.schedule(task, Message::AddSource)
    }
}

/// Return value from the `LoopHandle::add_timeout` method, a future that will
/// resolve to a `TimeoutToken` to configure the behavior of that timeout.
pub struct AddTimeout {
    inner: LoopFuture<TimeoutToken, Instant>,
}

/// A token that identifies an active timeout.
pub struct TimeoutToken {
    token: usize,
}

impl Future for AddTimeout {
    type Item = TimeoutToken;
    type Error = io::Error;

    fn poll(&mut self, _task: &mut Task) -> Poll<TimeoutToken, io::Error> {
        self.inner.poll(Loop::add_timeout)
    }

    fn schedule(&mut self, task: &mut Task) {
        self.inner.schedule(task, Message::AddTimeout)
    }
}

struct LoopFuture<T, U> {
    loop_handle: LoopHandle,
    data: Option<U>,
    result: Option<(Arc<Slot<io::Result<T>>>, slot::Token)>,
}

impl<T, U> LoopFuture<T, U>
    where T: Send + 'static,
{
    fn poll<F>(&mut self, f: F) -> Poll<T, io::Error>
        where F: FnOnce(&Loop, U) -> io::Result<T>,
    {
        match self.result {
            Some((ref result, ref token)) => {
                result.cancel(*token);
                match result.try_consume() {
                    Ok(t) => t.into(),
                    Err(_) => Poll::NotReady,
                }
            }
            None => {
                let data = &mut self.data;
                self.loop_handle.with_loop(|lp| {
                    match lp {
                        Some(lp) => f(lp, data.take().unwrap()).into(),
                        None => Poll::NotReady,
                    }
                })
            }
        }
    }

    fn schedule<F>(&mut self, task: &mut Task, f: F)
        where F: FnOnce(U, Arc<Slot<io::Result<T>>>) -> Message,
    {
        if let Some((ref result, ref mut token)) = self.result {
            result.cancel(*token);
            let handle = task.handle().clone();
            *token = result.on_full(move |_| {
                handle.notify();
            });
            return
        }

        let handle = task.handle().clone();
        let result = Arc::new(Slot::new(None));
        let token = result.on_full(move |_| {
            handle.notify();
        });
        self.result = Some((result.clone(), token));
        self.loop_handle.send(f(self.data.take().unwrap(), result))
    }
}

impl TimeoutState {
    fn block(&mut self, handle: TaskHandle) -> Option<TaskHandle> {
        match *self {
            TimeoutState::Fired => return Some(handle),
            _ => {}
        }
        *self = TimeoutState::Waiting(handle);
        None
    }

    fn fire(&mut self) -> Option<TaskHandle> {
        match mem::replace(self, TimeoutState::Fired) {
            TimeoutState::NotFired => None,
            TimeoutState::Fired => panic!("fired twice?"),
            TimeoutState::Waiting(handle) => Some(handle),
        }
    }
}

impl<E> Source<E> {
    pub fn new(e: E) -> Source<E> {
        Source {
            readiness: AtomicUsize::new(0),
            io: e,
        }
    }
}

impl<E: ?Sized> Source<E> {
    pub fn take_readiness(&self) -> Option<Ready> {
        match self.readiness.swap(0, Ordering::SeqCst) {
            0 => None,
            1 => Some(Ready::Read),
            2 => Some(Ready::Write),
            3 => Some(Ready::ReadWrite),
            _ => panic!(),
        }
    }

    pub fn io(&self) -> &E {
        &self.io
    }
}
