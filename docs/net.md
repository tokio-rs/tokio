# Non-blocking I/O

This section describes the network resources and driver provided by Tokio. This
component provide one of Tokio's primary functions: non-blocking, event-driven,
networking provided by the appropriate operating system primitives (epoll,
kqueue, IOCP, etc...). It is modeled after the resource and driver pattern
described in the previous section.

The network driver is built using [mio] and network resources are backed by
types that implement [`Evented`].

This guide will be focused on TCP types. The other network resources (UDP, unix
sockets, pipes, etc) follow the same pattern.

## The network resource.

Network resources are types, such as [`TcpListener`] and [`TcpStream`] are
composed of the network handle and a reference to the driver that is powering
the resource. Initially, when the resource is first created, the driver pointer
may be `None`:

```rust
let listener = TcpListener::bind(&addr).unwrap();
```

In this case, the reference to the driver is not yet set. However, if a
constructor that takes a [`Handle`] reference is used, then the driver reference
will be set to driver represented by the given handle.

Once a driver is associated with a resource, it is set for the lifetime of the
resource and cannot be changed. The associated driver is responsible for
receiving operating system events for the network resource and notifying the
tasks that have expressed interest in the resource.

### Using the resource

Resource types include non-blocking functions that are prefixed with `poll_` and
that include `Async` in the return type. These are the functions that are linked
with the task system and should be used from tasks and are used as part of
[`Future`] implementations. For example, `TcpStream` provides [`poll_read`] and
[`poll_write`]. [`TcpListener`] provides [`poll_accept`].

Here is a task that uses [`poll_accept`] to accept inbound sockets from a
listener and handle them by spawning a new task:

```rust
struct Acceptor {
    listener: TcpListener,
}

impl Future for Acceptor {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        loop {
            let (socket, _) = try_ready!(self.listener.poll_accept());

            // Spawn a task to process the socket
            tokio::spawn(process(socket));
        }
    }
}
```

Resource types may also include functions that return futures. These are
helpers that use the `poll_` functions to provide additional functionality. For
example, [`TcpStream`] provides a [`connect`] function that returns a future.
This future will complete once the [`TcpStream`] has established a connection
with a peer (or failed attemepting to do so).

Using combinators to connect a [`TcpStream`]:

```rust
tokio::spawn({
    let connect_future = TcpStream::connect(&addr);

    connect_future
        .and_then(|socket| process(socket))
});
```

Futures may also be used directly from other future implementations:

```rust
struct ConnectAndProcess {
    connect: ConnectFuture,
}

impl Future for ConnectAndProcess {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        let socket = try_ready!(self.connect.poll());
        tokio::spawn(process(socket));
        Ok(Async::Ready(()))
    }
}
```

## Registering the resource with the driver

When using [`TcpListener::poll_accept`][poll_accept] (or any `poll_*` function),
if the resource is ready to return immediately then it will do so. In the case
of [`poll_accept`][poll_accept], being ready means that there is a socket
waiting to be accepted in the queue. If the resource is **not** ready, i.e.
there is no pending socket to accept, then the resource asks the driver to
notify the current task once it becomes ready.

The first time `NotReady` is returned by a resource, if the resource was not
explicity assigned a driver using a [`Handle`] argument, the resource will register
itself with a driver instance. This is done by looking at the network driver
associated with the current execution context.

The default driver for the execution context is stored using a thread-local, set
using [`with_default`], and accessed using [`Handle::current`]. It is the
runtime's responsibility to ensure that the task is polled from within the
closure passed to [`with_default`]. A call to [`Handle::current`] accesses the
thread-local set by [`with_default`] in order to return the handle to the
driver for the curreent execution context.

### `Handle::current` vs `Handle::default`

Both `Handle::current` and `Handle::default` return a `Handle` instance.
They are, however, subtily different. `Handle::current` **immediately** reads
the thread-local variable storing the driver for the current driver. This means
that `Handle::current` must be called from an execution context that set the
default driver.

On the other hand, [`Handle::default`] lazily reads the thread-local variable.
This allows getting a `Handle` instance from *outside* of an execution context.
When the resource is used, the handle will access the thread-local variable as
described in the previous section.

For example:

```rust
fn main() {
    let std_listener = ::std::net::TcpListener::bind(&addr);
    let listener = TcpListener::from_std(std_listener, &Handle::default());

    tokio::run({
        listener.incoming().for_each(|socket| {
            tokio::spawn(process(socket));
            Ok(())
        })
    });
}
```

In this example, `incoming()` returns a future that is implemented by calling
`poll_accept`. The future is spawned onto a runtime, which has a network driver
configured as part of the execution context. When `poll_accept` is called from
within the execution context, that is when the thread-local is read and the
driver is associated with the `TcpListener` instance.

[`TcpStream`]: https://docs.rs/tokio/0.1/tokio/net/struct.TcpStream.html
[`TcpListener`]: https://docs.rs/tokio/0.1/tokio/net/struct.TcpListener.html
[`Handle`]: https://docs.rs/tokio-reactor/0.1/tokio_reactor/struct.Handle.html
[poll_accept]: http://docs.rs/tokio/0.1.8/tokio/net/struct.TcpListener.html#method.poll_accept
[`with_default`]: https://docs.rs/tokio-reactor/0.1.5/tokio_reactor/fn.with_default.html
[`Handle::default`]: https://docs.rs/tokio-reactor/0.1.5/tokio_reactor/struct.Handle.html#method.default

## The network driver

The driver that powers all of Tokio's network types is the [`Reactor`] type in
the [`tokio-reactor`] crate. It is implemented using [mio]. Calls to
[`Reactor::turn`] uses [`mio::Poll::poll`] to get operating system events for
registered network resources. It then notifies the registered tasks for each
network resource using the [task system]. The tasks then get scheduled to run on
their associated executors and the task then sees the network resource as ready
and calls to `poll_*` functions return `Async::Ready`.

### Linking the driver with resources

The driver must track each resource that is registered with it. While the actual
implementation is more complex, it can be thought as a shared reference to a
cell sharing state, similar to:

```rust
struct Registration {
    // The registration needs to know its ID. This allows it to remove state
    // from the reactor when it is droppeed.
    id: Id,

    // The task that owns the resource and is registered to receive readiness
    // notifications from the driver.
    //
    // If `task` is `Some`, we **definitely** know that the resource
    // is not ready because we have not yet received an operating system event.
    // This allows avoiding syscalls that will return `NotReady`.
    //
    // If `task` is `None`, then the resource **might** be ready. We can try the
    // syscall, but it might still return `NotReady`.
    task: Option<task::Task>,
}

struct TcpListener {
    mio_listener: mio::TcpListener,
    registration: Option<Arc<Mutex<Registration>>>,
}

struct Reactor {
    poll: mio::Poll,
    resources: HashMap<Id, Arc<Mutex<Registration>>>,
}
```

**This is not the real implementation**, but a simplified version to demonstrate
the behavior. In practice, there is no `Mutex`, cells are not allocated per
resource instance, and the reactor does not use a `HashMap`.

When the resource is first used, it is registered with the driver:

```rust
fn poll_accept(&mut self) -> Poll<TcpStream, io::Error> {
    // If the registration is not set, this will associate the `TcpListener`
    // with the current execution context's reactor.
    let registration = self.registration.get_or_insert_with(|| {
        // Access the thread-local variable that tracks the reactor.
        Reactor::with_current(|reactor| {
            // Registers the listener, which implements `mio::Evented`.
            // `register` returns the registration instance for the resource.
            reactor.register(&self.mio_listener)
        })
    });

    if registration.task.is_none() {
        // The task is `None`, this means the resource **might** be ready.
        match self.mio_listener.accept() {
            Ok(socket) => {
                let socket = mio_socket_to_tokio(socket);
                return Ok(Async::Ready(socket));
            }
            Err(ref e) if e.kind() == WouldBlock => {
                // The resource is not ready, fall through to task registration
            }
            Err(e) => {
                // All other errors are returned to the caller
                return Err(e);
            }
        }
    }

    // The task is set even if it is already `Some`, this handles the case where
    // the resource is moved to a different task than the one stored in
    // `self.task`.
    registration.task = Some(task::current());
    Ok(Async::NotReady)
}
```

Note that there is only a single `task` field per resource. The implications are
that a resource can only be used from a single task at a time. If
`TcpListener::poll_accept` returns `NotReady`, registering the current task and
the listener is then sent to a different task which calls `poll_accept` and sees
`NotReady`, then the second task is the only one that will receive a
notification once a socket is ready to be accepted. Resources may support
tracking different tasks for different operations. For example, `TcpStream`
internally has two task fields: one for notifying on read ready and one for
notifying on write ready. This allows `TcpStream::poll_read` and
`TcpStream::poll_write` to be called from different tasks.

The evented types are registered with the driver's mio::Poll instance as part of
the `register` function used above (again, this guide uses a **simplified**
implementation which does not match the actual one in `tokio-reactor` but is
sufficient for understanding how `tokio-reactor` behaves).

```rust
fn register<T: mio::Evented>(&mut self, evented: &T) -> Arc<Mutex<Registration>> {
    // Generate a unique identifier for this registration. This identifier
    // can be converted to and from a Mio Token.
    let id = generate_unique_identifier();

    // Register the I/O type with Mio
    self.poll.register(
        evented, id.into_token(),
        mio::Ready::all(),
        mio::PollOpt::edge());

    let registration = Arc::new(Mutex::new(Registration {
        id,
        task: None,
    }));

    self.resources.insert(id, registration.clone());

    registration
}
```

### Running the driver

The driver needs to run in order for its associated resources to function. If
the driver does not run, the resources will never become ready. Running the
driver is handled automatically when using a Runtime, but it is useful to
understand how it works. If you are interested in the real implementation, the
`tokio-reactor` source is the best reference.

When resources are registered with the driver, they are also registered with
Mio. Running the driver performs the following steps in a loop:

1) Call `Poll::poll` to get operating system events.
2) Dispatch all events to the appropriate resources via the registration.

The steps above are done by calling `Reactor::turn`. The looping part is up to
us.

```rust
loop {
    // `None` means never timeout, blocking until we receive an operating system
    // event.
    reactor.turn(None);
}
```

The implementation of `turn` does the following:

```rust
fn turn(&mut self) {
    // Create storage for operating system events. This shouldn't be created
    // each time `turn` is called, but doing so does not impact behavior.
    let mut events = mio::Events::with_capacity(1024);

    self.poll.poll(&mut events, timeout);

    for event in &events {
        let id = Id::from_token(event.token());

        if let Some(registration) = self.resources.get(&id) {
            if let Some(task) = registration.lock().unwrap().task.take() {
                task.notify();
            }
        }
    }
}
```

Notifying the task results in the task getting scheduled on its executor. When
the task runs again, it will call the `poll_accept` function again. This time,
the `task` slot will be `None`. This means the syscall should be attempted, and
this time `poll_accept` will return an accepted socket (probably, spurious events are permitted).
