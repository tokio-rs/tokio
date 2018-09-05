# Tokio Architecture

This document provides an in depth guide of the internals of Tokio, the various
components, and how they fit together. It expects that the reader already has a
fairly good understanding of how to use Tokio. A [Getting started][guide] guide
is provided on the website.

[guide]: https://tokio.rs/docs/getting-started/hello-world

## Overview

  * Tasks
  * Executors
  * Resources
  * Drivers

## Runtime model

Applications written using Tokio are organized across a large number of small,
non-blocking, tasks. A Tokio task is similar to a [goroutine][goroutine] or an
[erlang process][erlang], but is non-blocking. They are designed to be
lightweight, can be spawned fast, and maintain low scheduling overhead. They are
also non-blocking, as such operations that are not able to finish immediately
must still return immediately. Instead of returning the result of the operation,
they return a value indicating that the operation is in progress.

[goroutine]: https://www.golang-book.com/books/intro/10#section1
[erlang]: http://erlang.org/doc/reference_manual/processes.html

### Non-Blocking

A Tokio task is implemented using the [`Future`][Future] trait:

```rust
struct MyTask {
    my_resource: MyResource,
}

impl Future for MyTask {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        match self.my_resource.poll()? {
            Ok(Async::Ready(value)) => {
                self.process(value);
                Ok(Async::Ready(()))
            }
            Ok(Async::NotReady) => Ok(Async::NotReady),
            Err(err) => {
                self.process_err(err);
                Ok(Async::Ready(()))
            }
        }
    }
}
```

Tasks are submitted to an executor using `tokio::spawn` or by calling a `spawn`
method on an executor object. The `poll` function drives the task. No work is
done without calling `poll`. It is the executor's job to call `poll` on the task
until `Ready(())` is returned.

`MyTask` will receive a value from `my_resource` and process it. Once the value
has been processed, the task has completed its logic and is done. This is
represented by returning `Ok(Async::Ready(()))`.

However, in order to complete processing, the task depends on `my_resource`
providing a value. Given that `my_resource` is a non-blocking task, it may or
may not be ready to provide the value when `my_resource.poll()` is called. If it
is ready, it returns `Ok(Async::Ready(value))`. If it is not ready, it returns
`Ok(Async::NotReady)`.

When the resource is not ready to provide a value, this implies that the task
itself is not ready to complete and the task's `poll` function returns
`NotReady` as well.

At some point in the future, the resource will become ready to provide the
value. The resource uses the task system to signal to the executor that it is
ready. The executor schedules the task, which leads to `MyTask::poll` being
called again. This time, given that `my_resource` is ready, the value will be
returned from `my_resource.poll()` and the task is able to complete.

[Future]: https://docs.rs/futures/0.1/futures/future/trait.Future.html

#### Cooperative scheduling

Cooperative scheduling is used to schedule tasks on executors. A single executor
is expected to manage many tasks across a small set of threads. There will be
a far greater number of tasks then threads. There also is no pre-emption. This
means that when a task is scheduled to execute, it blocks the current thread
until the `poll` function returns.

Because of this, it is important for implementations of `poll` to only execute
for very short periods of time. For I/O bound applications, this usually happens
automatically. However, if a task must run a longer computation, it should defer
work to a [blocking pool] or breakup the computation into smaller chunks and
[yield] back to the executor after each chunk.

[blocking pool]: #
[yield]: #

### Task system

The task system is the system by which resources notify executors of readiness
changes. A task is composed of non-blocking logic that consume resources. In the
example above, `MyTask` uses a single resource, `my_resource`, but there is no
limit to the number of resources that a task can consume.

When a task is executing and attemepts to use a resource that is not ready, it
becomes *logically* blocked on that resource, i.e., the task is not able to make
further progress until that resource becomes ready. Conversely, if a task does
not attempt to consume a resource, it is not blocked on that resource, i.e.,
thee readiness of that resource does not impact the ability of the task to make
forward progress.

Tokio tracks which resources a task is currently blocked on to make forward
progress. When a dependent resource becomes ready, the executor schedules the
task. This is done by tracking when a task **expresses interest** in a resource.

When `MyTask` executes, attempts to consume `my_resource`, and `my_resource`
returns `NotReady`, `MyTask` has implicitly expressed interest in the
`my_resource` resource. At this point the task and the resource are linked. When
the resource becomes ready, the task is scheduled again.

#### `task::current` and `Task::notify`

Tracking interest and notifying readiness changes is done with two APIs:

  * [`task::current`][current]
  * [`Task::notify`][notify]

When `my_resource.poll()` is called, if the resource is ready, it immediately
returns the value without using the task system. If the resource is **not**
ready, it gets a handle to the current task by calling [`task::current() ->
Task`][current]. This handle is obtained by reading a thread-local variable set
by the executor.

Some external event (data received on the network, background thread completing
a computation, etc...) will result in `my_resource` becoming ready to produce
its value. At that point, the logic that readies `my_resource` will call
[`notify`] on the task handle obtained from [`task::current`][current]. This
signals the readiness change to the executor, which then schedules the task for
execution.

#### `Async::Ready`

Any function that returns `Async` must adheare to the [contract][contract]. When
`NotReady` is returned, the current task **must** have been registered for
notification on readiness change. The implication for resources is discussed in
the above section. For task logic, this means that `NotReady` cannot be returned
unless a resource has returned `NotReady`. By doing this, the
[contract][contract] transitively upheld. The current task is registered for
notification because `NotReady` has been received from the resource.

Great care must be taken to avoiding returning `NotReady` without having
received `NotReady` from a resource. For example, the following task
implementation results in the task never completing.

```rust

enum BadTask {
    First(Resource1),
    Second(Resource2),
}

impl Future for BadTask {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        use self::BadTask::*;

        match *self {
            First(ref mut resource) => {
                let value = try_ready!(resource.poll());
                *self = Second(Resource2::new(value));
                Ok(Async::NotReady)
            }
            Second(ref mut resource) => {
                try_ready!(resource.poll());
                Ok(Async::Ready(()))
            }
        }
    }
}
```

The problem with the above implementation is that `Ok(Async::NotReady)` is
returned right after transitioning the state to `Second`. During this
transition, no resource has returned `NotReady`. When the task itself returns
`NotReady`, it has violated the [contract][contract] as the task will **not** be
notified in the future.

This situation is generally resolved by adding a loop:

```rust
fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
    use self::BadTask::*;

    loop {
        match *self {
            First(ref mut resource) => {
                let value = try_ready!(resource.poll());
                *self = Second(Resource2::new(value));
            }
            Second(ref mut resource) => {
                try_ready!(resource.poll());
                return Ok(Async::Ready(()));
            }
        }
    }
}
```

One way to think about it is that a task's `poll` function **must not**
returned until it is unable to make any further progress due to its resources
not being ready.

##### Yielding

Somtimes a task must return `NotReady` without being blocked on a resource. This
usually happens when computation to run is large and the task wants to return
control to the executor to allow it to execute other futures.

Yielding is done by notifying the current task and returning `NotReady`:

```rust
task::current().notify();
return Ok(Async::NotReady);
```

Yield can be used to break up a CPU expensive computation:

```rust
struct Count {
  remaining: usize,
}

impl Future for Count {
    type Item = ();
    type Error = ();

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        while self.count > 0 {
            self.count -= 1;

            // Yield every 10 iterations
            if self.count % 10 == 0 {
                task::current().notify();
                return Ok(Async::NotReady);
            }
        }

        Ok(Async::Ready(()))
    }
}
```

[contract]: https://docs.rs/futures/0.1.23/futures/future/trait.Future.html#tymethod.poll

### Executors

Executors are responsible for driving many tasks to completion. A task is
spawned onto an executor, at which point the executor calls its `poll` function
when needed. The executor hooks into the task system to receive resource
readiness notifications.

By decoupling the task system with the executor implementation, the specific
execution and scheduling logic can be left to the executor implementation. Tokio
provides two executor implementations, each with unique characteristics:
[`current_thread`] and [`thread_pool`].

When a task is first spawned onto the executor, the executor wraps it w/
[`Spawn`][Spawn]. This binds the task logic with the task state (this is mostly
required for legacy reasons). When the executor picks a task for execution, it
calls [`Spawn::poll_future_notify`][poll_future_notify]. This function ensures
that the task context is set to the thread-local variable such that
[`task::current`][current] is able to read it. When calling
[`poll_future_notify`][poll_future_notify], the executor also passes in a notify
handle and an identifier. These arguments are included in the task handle
returned by [`task::current`][current] and is how the task is linked to the
executor.

The notify handle is an implementation of [`Notify`][`Notify`] and the identifier
is a value that the executor uses to look up the current task. When
[`Task::notify`][notify] is called, the [`notify`][Notify::notify] function on
the notify handle is called with the supplied identifier. The implementation of
this function is responsible for performing the scheduling logic.

One strategy for implementing an executor is to store each task in a `Box` and
to use a linked list to track tasks that are scheduled for execution. When
[`Notify::notify`][Notify::notify] is called, then the task associated with the
identifier is pushed at the end of the `scheduled` linked list. When the
executor runs, it pops from the front of the linked list and executes the task
as described above.

Note that this section does not describe how the executor is run. The details of
this are left to the executor implementation. One option is for the executor to
spawn one or more threads and dedicate these threads to draining the `scheduled`
linked list. Another is to provide a `MyExecutor::run` function that blocks the
current thread and drains the `scheduled` linked list.

[`current_thread`]: #
[`thread_pool`]: #
[Spawn]: https://docs.rs/futures/0.1/futures/executor/struct.Spawn.html
[poll_future_notify]: https://docs.rs/futures/0.1/futures/executor/struct.Spawn.html#method.poll_future_notify
[current]: https://docs.rs/futures/0.1/futures/task/fn.current.html
[notify]: https://docs.rs/futures/0.1/futures/task/struct.Task.html#method.notify
[`Notify`]: https://docs.rs/futures/0.1/futures/executor/trait.Notify.html
[Notify::notify]: https://docs.rs/futures/0.1/futures/executor/trait.Notify.html#tymethod.notify

### Resources, Drivers, and Runtimes

Resources are leaf futures, i.e. futures that are not implemented in terms of
other futures. They are the types that use the task system described above to
interact with the executor. Resource types include TCP and UDP sockets, timers,
channels, file handles, etc. Tokio applications rarely need to implement
resources. Instead, they use resources provided by Tokio or third party crates.

Often times, a resource cannot function by itself and requires a driver. For
example, Tokio TCP sockets are backed by a [`Reactor`]. The reactor is the
socket resource driver. A single driver may power large numbers of resource
instances. In order to use the resource, the driver must be running somewhere in
the process. Tokio provides drivers for network resources ([`tokio-reactor`]),
file resources ([`tokio-fs`]), and timers ([`tokio-timer]`). Providing decoupled
driver components allows useers to pick and choose which components they wish to
use. Each driver can be used standalone or combined with other drivers.

Because of this, in order to use Tokio and successfully execute tasks, an
application must start an executor and the necessary drivers for the resources
that the application's tasks depend on. This requires significant boilerplate.
To manage the boilerplate, Tokio offers a couple runtime options. A runtime is
an executor bundled with all necessary drivers to power Tokio's resources.
Instead of managing all the various Tokio components individually, a runtime is
created and started in a single call.

Tokio offers a [concurrent runtime][concurrent], backed by a multi-threaded, work-stealing,
executor as well as a runtime that runs the executor and all drivers on the
[current thread][current_thread]. This allows the user to pick the runtime
characteristics best suited for the application.

[`Reactor]: https://docs.rs/tokio-reactor/0.1.5/tokio_reactor/
[`tokio-reactor`]: https://docs.rs/tokio-reactor
[`tokio-fs`]: https://docs.rs/tokio-fs
[`tokio-timer`]: https://docs.rs/tokio-timer
[concurrent]: https://docs.rs/tokio/0.1.8/tokio/runtime/index.html
[current_thread]: https://docs.rs/tokio/0.1.8/tokio/runtime/current_thread/index.html

### Future

As mentioned above, tasks are implemented using the [`Future`] trait. This trait
is not limited to implementing tasks. A [`Future`] is a value that represents a
non-blocking computation that will complete sometime in the future. A task is a
computation with no output. Many resources in Tokio are represented with
[`Future`] implementations. For example, a timeout is a [`Future`] that
completes once the deadline has been reached.

The trait includes a number of combinators that are useful for working with
future values.

Applications are built by either implementing `Future` for application specific
types or defining application logic using combinators. Often, a mix of both
strategies is most successful.

<!-- TODO: Expand -->

[`Future`]: https://docs.rs/futures/0.1/futures/future/trait.Future.html

## Reactor

## Runtime
