This directory is for running one of the main benchmarks for
[noria](https://github.com/mit-pdos/noria), a highly concurrent
research-prototype database that uses tokio. This is intended as a macro
benchmark that covers many aspects of tokio's performance profile.

## To run

```console
$ ./run.sh
# go get tea/coffee
$ ls noria-*.log
```

## To understand

The underlying application is quite complex, and it can be difficult to
figure out exactly how changes to tokio affect the benchmark
performance. Similarly, it can be difficult to diagnose the root cause
of performance problems. Nevertheless, here are some pointers into the
code that may be useful.

First of all, the benchmark is "[open
loop](https://www.usenix.org/legacy/event/nsdi06/tech/full_papers/schroeder/schroeder.pdf)",
meaning it issues requests independently of whether outstanding requests
have completed. This allows you to accurately measure latency spikes
(see the link for details). The general idea of such a benchmark is that
you run it with a given target throughput, and you see what the latency
is. You then keep increasing the target throughput until the latency
starts to spike. When the latency spikes, the system is no longer
keeping up, and you've found the peak supported load.

The benchmark consists of two "halves", the benchmark client and the
server. The client runs some number of "load generator" threads (you'll
see these hit ~100% CPU usage during the benchmark as they spin), and
issues batches of requests and measures their latency. The client
requests are multiplexed over a dynamically-sized [connection
pool](https://docs.rs/tower-balance/0.3.0/tower_balance/pool/index.html).
Each batch is either a batch of reads or a batch of writes.

On the server side, reads and writes arrive at different ports, and are
handled by different code paths. Writes go to a "replica", which is the
giant hand-written future in `noria-server/src/worker/replica.rs`. When
the benchmark is run with `--shards 0`, there is only one, whereas if it
is run with `--shards N`, there are `N` of them. Load will be mostly
uniformly spread across the `N` replicas.

Reads go through the `handle_message` method in
`noria-server/src/worker/readers.rs`, and often complete immediately.
Some reads may miss in Noria's internal cache, and have to wait for that
cache entry to be populated. It does this by "triggering a replay",
which involves sending a message over a channel to one of the replicas
from above (see the `Replica` field `locals`). It then spawns a
`BlockingRead` future which retries the reads that did not previously
complete on a timer. When sharding is enabled, a single client read will
be spread across `N` independent calls to `handle_message`, and the
client request completes when all of them have completed.

The code makes somewhat heavy use of tokio's `block_in_place`.
Specifically, it is used to handle every write and every "triggered
replay". This may or may not be relevant.
