//! Event loop that drives Tokio I/O resources.
//!
//! This module contains [`Reactor`], which is the event loop that drives all
//! Tokio I/O resources. It is the reactor's job to receive events from the
//! operating system ([epoll], [kqueue], [IOCP], etc...) and forward them to
//! waiting tasks. It is the bridge between operating system and the futures
//! model.
//!
//! # Overview
//!
//! When using Tokio, all operations are asynchronous and represented by
//! futures. These futures, representing the application logic, are scheduled by
//! an executor (see [runtime model] for more details). Executors wait for
//! notifications before scheduling the future for execution time, i.e., nothing
//! happens until an event is received indicating that the task can make
//! progress.
//!
//! The reactor receives events from the operating system and notifies the
//! executor.
//!
//! Let's start with a basic example, establishing a TCP connection.
//!
//! ```rust
//! # extern crate tokio;
//! # fn dox() {
//! use tokio::prelude::*;
//! use tokio::net::TcpStream;
//!
//! let addr = "93.184.216.34:9243".parse().unwrap();
//!
//! let connect_future = TcpStream::connect(&addr);
//!
//! let task = connect_future
//!     .and_then(|socket| {
//!         println!("successfully connected");
//!         Ok(())
//!     })
//!     .map_err(|e| println!("failed to connect; err={:?}", e));
//!
//! tokio::run(task);
//! # }
//! # fn main() {}
//! ```
//!
//! Establishing a TCP connection usually cannot be completed immediately.
//! [`TcpStream::connect`] does not block the current thread. Instead, it
//! returns a [future][connect-future] that resolves once the TCP connection has
//! been established. The connect future itself has no way of knowing when the
//! TCP connection has been established.
//!
//! Before returning the future, [`TcpStream::connect`] registers the socket
//! with a reactor. This registration process, handled by [`Registration`], is
//! what links the [`TcpStream`] with the [`Reactor`] instance. At this point,
//! the reactor starts listening for connection events from the operating system
//! for that socket.
//!
//! Once the connect future is passed to [`tokio::run`], it is spawned onto a
//! thread pool. The thread pool waits until it is notified that the connection
//! has completed.
//!
//! When the TCP connection is established, the reactor receives an event from
//! the operating system. It then notifies the thread pool, telling it that the
//! connect future can complete. At this point, the thread pool will schedule
//! the task to run on one of its worker threads. This results in the `and_then`
//! closure to get executed.
//!
//! ## Lazy registration
//!
//! Notice how the snippet above does not explicitly reference a reactor. When
//! [`TcpStream::connect`] is called, it registers the socket with a reactor,
//! but no reactor is specified. This works because the registration process
//! mentioned above is actually lazy. It doesn't *actually* happen in the
//! [`connect`] function. Instead, the registration is established the first
//! time that the task is polled (again, see [runtime model]).
//!
//! A reactor instance is automatically made available when using the Tokio
//! [runtime], which is done using [`tokio::run`]. The Tokio runtime's executor
//! sets a thread-local variable referencing the associated [`Reactor`] instance
//! and [`Handle::current`] (used by [`Registration`]) returns the reference.
//!
//! ## Implementation
//!
//! The reactor implementation uses [`mio`] to interface with the operating
//! system's event queue. A call to [`Reactor::poll`] results in a single
//! call to [`Poll::poll`] which in turn results in a single call to the
//! operating system's selector.
//!
//! The reactor maintains state for each registered I/O resource. This tracks
//! the executor task to notify when events are provided by the operating
//! system's selector. This state is stored in a `Sync` data structure and
//! referenced by [`Registration`]. When the [`Registration`] instance is
//! dropped, this state is cleaned up. Because the state is stored in a `Sync`
//! data structure, the [`Registration`] instance is able to be moved to other
//! threads.
//!
//! By default, a runtime's default reactor runs on a background thread. This
//! ensures that application code cannot significantly impact the reactor's
//! responsiveness.
//!
//! ## Integrating with the reactor
//!
//! Tokio comes with a number of I/O resources, like TCP and UDP sockets, that
//! automatically integrate with the reactor. However, library authors or
//! applications may wish to implement their own resources that are also backed
//! by the reactor.
//!
//! There are a couple of ways to do this.
//!
//! If the custom I/O resource implements [`mio::Evented`] and implements
//! [`std::io::Read`] and / or [`std::io::Write`], then [`PollEvented`] is the
//! most suited.
//!
//! Otherwise, [`Registration`] can be used directly. This provides the lowest
//! level primitive needed for integrating with the reactor: a stream of
//! readiness events.
//!
//! [`Reactor`]: struct.Reactor.html
//! [`Registration`]: struct.Registration.html
//! [runtime model]: https://tokio.rs/docs/getting-started/runtime-model/
//! [epoll]: http://man7.org/linux/man-pages/man7/epoll.7.html
//! [kqueue]: https://www.freebsd.org/cgi/man.cgi?query=kqueue&sektion=2
//! [IOCP]: https://msdn.microsoft.com/en-us/library/windows/desktop/aa365198(v=vs.85).aspx
//! [`TcpStream::connect`]: ../net/struct.TcpStream.html#method.connect
//! [`connect`]: ../net/struct.TcpStream.html#method.connect
//! [connect-future]: ../net/struct.ConnectFuture.html
//! [`tokio::run`]: ../runtime/fn.run.html
//! [`TcpStream`]: ../net/struct.TcpStream.html
//! [runtime]: ../runtime
//! [`Handle::current`]: struct.Handle.html#method.current
//! [`mio`]: https://github.com/carllerche/mio
//! [`Reactor::poll`]: struct.Reactor.html#method.poll
//! [`Poll::poll`]: https://docs.rs/mio/0.6/mio/struct.Poll.html#method.poll
//! [`mio::Evented`]: https://docs.rs/mio/0.6/mio/trait.Evented.html
//! [`PollEvented`]: struct.PollEvented.html
//! [`std::io::Read`]: https://doc.rust-lang.org/std/io/trait.Read.html
//! [`std::io::Write`]: https://doc.rust-lang.org/std/io/trait.Write.html

pub use tokio_reactor::{
    Reactor,
    Handle,
    Background,
    Turn,
    Registration,
    PollEvented as PollEvented2,
};

mod poll_evented;
#[allow(deprecated)]
pub use self::poll_evented::PollEvented;
