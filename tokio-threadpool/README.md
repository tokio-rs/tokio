# Tokio Thread Pool

A library for scheduling execution of futures concurrently across a pool of
threads.

**Note**: This library isn't quite ready for use.

### Why not Rayon?

Rayon is designed to handle parallelizing single computations by breaking them
into smaller chunks. The scheduling for each individual chunk doesn't matter as
long as the root computation completes in a timely fashion. In other words,
Rayon does not provide any guarantees of fairness with regards to how each task
gets scheduled.

On the other hand, `tokio-threadpool` is a general purpose scheduler and
attempts to schedule each task fairly. This is the ideal behavior when
scheduling a set of unrelated tasks.

### Why not futures-cpupool?

It's 10x slower.

## Examples

```rust
extern crate tokio_threadpool;
extern crate futures;

use tokio_threadpool::*;
use futures::*;
use futures::sync::oneshot;

pub fn main() {
    let (tx, _pool) = ThreadPool::new();

    let res = oneshot::spawn(future::lazy(|| {
        println!("Running on the pool");
        Ok::<_, ()>("complete")
    }), &tx);

    println!("Result: {:?}", res.wait());
}
```

## License

`tokio-threadpool` is primarily distributed under the terms of both the MIT
license and the Apache License (Version 2.0), with portions covered by various
BSD-like licenses.

See LICENSE-APACHE, and LICENSE-MIT for details.
