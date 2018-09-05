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

## Readiness notifications

wut wut wut


[`TcpStream`]: https://docs.rs/tokio/0.1/tokio/net/struct.TcpStream.html
[`TcpListener`]: https://docs.rs/tokio/0.1/tokio/net/struct.TcpListener.html
[`Handle`]: https://docs.rs/tokio-reactor/0.1/tokio_reactor/struct.Handle.html
