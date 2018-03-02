# tokio-executor

Task execution related traits and utilities.

[Documentation](https://tokio-rs.github.io/tokio/tokio_executor/)

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

* [`enter`] marks that the the current thread is entering an execution
  context. This prevents a second executor from accidentally starting from
  within the context of one that is already running.

* [`DefaultExecutor`] spawns tasks onto the default executor for the current
  context.

* [`Park`] abstracts over blocking and unblocking the current thread.

[`Executor`]: https://tokio-rs.github.io/tokio/tokio_executor/trait.Executor.html
[`enter`]: https://tokio-rs.github.io/tokio/tokio_executor/fn.enter.html
[`DefaultExecutor`]: https://tokio-rs.github.io/tokio/tokio_executor/struct.DefaultExecutor.html
[`Park`]: https://tokio-rs.github.io/tokio/tokio_executor/park/index.html

## License

This project is licensed under either of

 * Apache License, Version 2.0, ([LICENSE-APACHE](../LICENSE-APACHE) or
   http://www.apache.org/licenses/LICENSE-2.0)
 * MIT license ([LICENSE-MIT](../LICENSE-MIT) or
   http://opensource.org/licenses/MIT)

at your option.

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, as defined in the Apache-2.0 license, shall be
dual licensed as above, without any additional terms or conditions.
