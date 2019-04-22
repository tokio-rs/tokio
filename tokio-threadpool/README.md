# Tokio Thread Pool

A library for scheduling execution of futures concurrently across a pool of
threads.

[Documentation](https://docs.rs/tokio-threadpool/0.1.14/tokio_threadpool)

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

use tokio_threadpool::ThreadPool;
use futures::{Future, lazy};
use futures::sync::oneshot;

pub fn main() {
    let pool = ThreadPool::new();
    let (tx, rx) = oneshot::channel();

    pool.spawn(lazy(|| {
        println!("Running on the pool");
        tx.send("complete").map_err(|e| println!("send error, {}", e))
    }));

    println!("Result: {:?}", rx.wait());
    pool.shutdown().wait().unwrap();
}
```

## License

This project is licensed under the [MIT license](LICENSE).

### Contribution

Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in Tokio by you, shall be licensed as MIT, without any additional
terms or conditions.
