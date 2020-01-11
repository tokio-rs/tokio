# tokio-executor

Task execution related traits and utilities.

This crate is **deprecated in tokio 0.2.x** and has been moved and refactored
into various places in the [`tokio::runtime`] module of the [`tokio`] crate.

[`tokio::runtime`]: https://docs.rs/tokio/latest/tokio/runtime/index.html
[`tokio`]: https://docs.rs/tokio/latest/tokio/index.html

[Documentation](https://docs.rs/tokio-executor/0.1.9/tokio_executor)

## Overview

In the Tokio execution model, futures are lazy. When a future is created, no
work is performed. In order for the work defined by the future to happen, the
future must be submitted to an executor. A future that is submitted to an
executor is called a "task".

The executor is responsible for ensuring that [`Future::poll`] is called
whenever the task is [notified]. Notification happens when the internal state of
a task transitions from "not ready" to ready. For example, a socket might have
received data and a call to `read` will now be able to succeed.

This crate provides traits and utilities that are necessary for building an
executor, including:

* The [`Executor`] trait describes the API for spawning a future onto an
  executor.

* [`enter`] marks that the current thread is entering an execution
  context. This prevents a second executor from accidentally starting from
  within the context of one that is already running.

* [`DefaultExecutor`] spawns tasks onto the default executor for the current
  context.

* [`Park`] abstracts over blocking and unblocking the current thread.

[`Executor`]: https://docs.rs/tokio-executor/0.1.9/tokio_executor/trait.Executor.html
[`enter`]: https://docs.rs/tokio-executor/0.1.9/tokio_executor/fn.enter.html
[`DefaultExecutor`]: https://docs.rs/tokio-executor/0.1.9/tokio_executor/struct.DefaultExecutor.html
[`Park`]: https://docs.rs/tokio-executor/0.1.9/tokio_executor/park/trait.Park.html

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
