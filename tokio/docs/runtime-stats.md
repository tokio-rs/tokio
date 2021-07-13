# Tokio Runtime Stats

## Summary

This RFC proposes a new way to gather understanding from the Tokio runtime. Currently, the runtime does not expose any methods to understand what is happening under the hood. This provides a rough experience when deploying Tokio based applications into production where you would like to understand what is happening to your code. Via this RFC, we will propose a few methods to collect this data at different levels. Beyond what is proposed as implemenation in this RFC, we will also discuss other methods to gather the information a user might need to be successful with Tokio in a production environment.

There are two main types of stats that a runtime can expose. A per-runtime stats (eg `executor_load`, `fds_registered`) that are collected indepdently of the tasks running on the runtime. A per-task stats (eg `poll_duration`, `amount_polls`) that are collected and aggregated at the task level. This RFC will propose an implemenation for implementing per-runtime stats but will also mention methods to capture per-task stats.

A small note, the term `stats` is used instead of `metrics` because we are only concerned with exposing raw data rather than methods of aggregating and emiting that data.

## Motivation

When developing and writing Tokio applications, there are many forms of support, be it tutorials or discord. But when running these applications in production there is not much support. Users want to understand what is happening behind the scenes. What is my runtime up too? How can I optimize my application? This RFC intends to provide a foundation to answer these questions.

## Guide-level explanation

Runtime stats will be exposed via a struct that is attainable via the `tokio::runtime::Handle`. Calling `Handle::stats()` will return a reference counted struct that contains raw stat values. Through this, there will be a `tokio-metrics` crate that converts these raw stats into proper aggregated metrics that can be consumed by end user metrics collection systems like the `metrics` crate.

```rust=
// Handle that can be acquired via `Runtime` or via `Handle::current`.
let handle = Handle::current();
let stats = handle.stats();

// executor level stats.
let executor = stats.executor();

// per-worker stats via the executor.
for worker in executor.workers() {
    println!("{}", worker.steal_count());
}

println!("{}", executor.global_queue_depth());

// io driver stats.
let io = stats.io();

println!("{}", io.sources_count());

let timer = stats.timer();

println!("{}", timer.entry_count());
```

### Executor

The handle will provide stats for each worker thread or in the case of the single threaded executor will provide a single worker. This provides a detailed view into what each worker is doing. Providing the ability for the `tokio-metrics` crate to expose the stats aggregated as a single metric or as a per-worker metric.

The values will be updated in batch from the executor to avoid needing to stream the data on every action. This should amortize the cost by only needing to emit stats at a specific executor wall clock time. Where the executor wall clock time is determinetd by a single executor tick rather than actual system time. This allows the collectors to observe the time and the stats to determine how long certain executor cycles took. This removes the need to acquire the time during executor cycles.

Each worker will expose these stats, updated in batches:

- Amount of futures executed
- Amount of work steal attempts
- Amount of parks
- Amount of times local queue overflowed into the global queue
- Max local queue depth
- Min local queue depth
- Avg local queue depth
- Queue depth at time of batch emission
- Amount of executor ticks (loop iterations)
- Number of `block_in_place` tasks entered

The main goal of this implementation is to allow a user to run this metrics collection at all times in production with minimal overhead to their application. This would allow users to alarm on any regressions and track how the runtime is performing.

Some of the stats include min/max (specifically the queue depth stats) this is because the depth of the queues changes throughout the stats batch window. The value could start low, spike up during the middle of the window then come back down. To understand this behavior the executor stats module will aggregate the depth values to reduce the need to stream the values.

### Global queue

Since, the global inject queue is not attached to a single worker its length value can be sampled directly from collector thread. In addition, the global inject queue length is already tracked via an `AtomicUsize`, therefore this value can just be exposed via the `Stats` struct directly.

Stats exposed for the global inject queue:
- Queue length

### Blocking operations

The blocking pool already tracks the number of idle threads and the total number of threads. These values are currently within a shared mutex but can be moved to be `AtomicUsize` values and then shared with the `Stats` struct to be sampled by the collector. In addition, a counter that is incremented on each task execution will be included. All values will be streamed to the stats struct via atomics.

Stats from the blocking pool:
- Number of idle threads
- Number of threads
- Number of blocking task
- Blocking task queue depth

### Task

This implementation will avoid tracking stats/metrics at the task level due to the overhead required. This will instead be accomplished by the [tokio console](https://github.com/tokio-rs/console). This will allow the user to attach the console and take the performance hit when they want to explore issues in more detail.

### I/O driver

Unlike, the executor stats, stats coming from the I/O driver will be streamed directly to the `Stats` struct via atomics. Each value will be incremented (via `AtomicU64::fetch_add`) for each event.

List of stats provided from the I/O driver:
- Amount of compact
- Amount of "token dispatches" (aka ready events)
- Amount of fd currently registered with `io::Driver`
- Amount of read ready events
- Amount of write ready events

### Timer driver

Similar to the I/O driver the timer driver will stream stats in a similar fashion. These will be emitted via `AtomicU64::fetch_add` and be emitted on each occurance of the events below.

List of stats provided from the timer driver:
- Amount of timer entries (timer wheel depth)
- Amount of timers fired
- Amount of timers cancelled

### `tokio-metrics`

The `tokio-metrics` crate will provide aggregated metrics based on the `Stats` struct. This will include histograms and other useful aggregated forms of the stats that could be emitted by various metrics implementations. This crate is designed to provide the ability to expose the aggregated stats in an unstable `0.1` method outside of the runtime and allow the ability to iterate on how they are aggregated without the need to follow `tokio`'s strict versioning scheme.

The rest of this crate is out of scope for this RFC, since most of the implementation will happen ad-hoc outside of the main crate.

## Implementation details

### Batch streaming

To avoid any extra overhead in the executor loop, each worker will batch metrics into a `Core` local struct. These values will be incremented or sampled during regular executor cycles when certain operations happen like a work steal attempt or a pop from one of the queues.

The batches will be streamed via atomics to the stats struct directly. This will reduce any cross CPU work while the executor is running and amortize the cost of having to do cross CPU work. Batches will be sent before the executor attempts to park the thread. This will happen either when there is no work to be done or when the executor has hit the maintance tick. At this point before the thread will park, the executor will submit the batch. Generally, since parking is more expensive then submitting batches there should not be any added latency to the executor cycle in this process.

This then allows the collector to poll the stats on any interval. Thus, allowing it to drive its own timer to understand an estimate of the duration that a certain amount of ticks too, or how many times the the executor entered the park state.

### I/O and Timer implementations

Since, the two drivers (I/O and timer) that `tokio` provides are singtons within the runtime there is no need to iterate through their stats like the executor stats. In-addition, it is possible to stream the metrics directly from the driver events rather than needing to batch them like the executor.

# Appendix

## Appendix A: References

- https://hexdocs.pm/ex_erlstats/ExErlstats.html
- https://golang.org/doc/diagnostics
- http://erlang.org/doc/man/msacc.html
- http://ferd.github.io/recon/recon.html
- http://erlang.org/doc/man/erlang.html#statistics_scheduler_wall_time
- https://s3.us-east-2.amazonaws.com/ferd.erlang-in-anger/text.v1.1.0.pdf