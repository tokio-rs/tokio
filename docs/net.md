# Network resources and driver

This section describes the network resources and driver provided by Tokio. This
component provide one of Tokio's primary functions: non-blocking, event-driven,
networking provided by the appropriate operating system primitives (epoll,
kqueue, IOCP, etc...). It is modeled after the resource and driver pattern
mentioned above.

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


[`TcpStream`]: https://docs.rs/tokio/0.1/tokio/net/struct.TcpStream.html
[`TcpListener`]: https://docs.rs/tokio/0.1/tokio/net/struct.TcpListener.html
[`Handle`]: https://docs.rs/tokio-reactor/0.1/tokio_reactor/struct.Handle.html
[poll_accept]: http://docs.rs/tokio/0.1.8/tokio/net/struct.TcpListener.html#method.poll_accept
[`with_default`]: https://docs.rs/tokio-reactor/0.1.5/tokio_reactor/fn.with_default.html
[`Handle::default`]: https://docs.rs/tokio-reactor/0.1.5/tokio_reactor/struct.Handle.html#method.default
