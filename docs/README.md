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

[current]: https://docs.rs/futures/0.1/futures/task/fn.current.html
[notify]: https://docs.rs/futures/0.1/futures/task/struct.Task.html#method.notify

### Executors

Executors are responsible for driving many tasks to completion. A task is
spawned onto an executor, at which point the executor calls its `poll` function
when needed.

The specific execution and scheduling logic is left to the executor
implementation.

### Future
