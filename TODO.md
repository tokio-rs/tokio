Have a plan:

- futures
  - Streams
    - [Error semantics](https://github.com/alexcrichton/futures-rs/issues/206)
      - Plan is to keep `and_then` etc on streams as they are -- so a future error terminates the stream
      - Can always lift Result through future if needed

- New tokio-io crate
  - Move contents of tokio-core/io module
  - Add AsyncRead/Write
    - Inherit from Read/Write in std
    - Integrate with the bytes crate
    - Provide framing, etc
    - Change combiantors to require AsyncRead/Write bounds
    - Change upper-level crates to use tokio-io rather than tokio-core
    - Zeroing out memory
  - Change `EasyBuf` to just import from `bytes` crate
    - Should just be `BytesMut`
    - Resolves
      - [Move to bytes crate](https://github.com/tokio-rs/tokio-core/issues/68)
      - [Reconsider `Vec<u8>` being exposed](https://github.com/tokio-rs/tokio-core/issues/81)
    - Codec:
      - Switch `In`/`Out` to `Encode` and `Decode`
      - Do not split the trait
      - [Revisit EOF](https://github.com/tokio-rs/tokio-core/issues/67)
        - We don't want to do anything with `Codec` here
        - Can add defaulted method to `Transport` as a hook for closing out the write side
      - [Allow error to be customized](https://github.com/tokio-rs/tokio-core/issues/146)
        - WONTFIX; I/O layer uses I/O error, period
      - Resolves
        - [Rename `In`/`Out`](https://github.com/tokio-rs/tokio-core/issues/135), [mark as `'static`](https://github.com/tokio-rs/tokio-core/issues/83)
  - Resolves:
    - [Possible separate traits crate](https://github.com/tokio-rs/tokio-core/issues/119) (see below)
    - [`AsyncRead`/`AsyncWrite` story](https://github.com/tokio-rs/tokio-core/issues/62)
      - [Relation to `std` traits](https://github.com/tokio-rs/tokio-core/issues/61)

- tokio-proto
  - [`bind_client` should return a future](https://github.com/tokio-rs/tokio-proto/issues/132)

Need discussion:

- futures
  - Parking/task model
    - [Panic in unpark](https://github.com/alexcrichton/futures-rs/issues/318)
    - [Make task context optional](https://github.com/alexcrichton/futures-rs/issues/131)
    - [Make task context explicit](https://github.com/alexcrichton/futures-rs/issues/129)
    - [Naming around `poll` for clarity](https://github.com/alexcrichton/futures-rs/issues/222); [other conventions](https://github.com/alexcrichton/futures-rs/issues/250)
  - Streams
    - [Returning ownership](https://github.com/alexcrichton/futures-rs/issues/301)
      - Can we integrate this into the core stream abstraction, to avoid a split?

- tokio-core
  - Global/default/ambient event loop
    - [Getting a handle](https://github.com/tokio-rs/tokio-core/issues/79); [PR](https://github.com/tokio-rs/tokio-core/pull/84)
    - Also for pure clients (like Hyper)

- tokio-service
  - [Backpressure](https://github.com/tokio-rs/tokio-service/issues/12)
    - [Carl's gist](https://gist.github.com/carllerche/5570c2b0e8a4994fb4a849020a5046de)
  - [The need for `Service` at all/relationship to `Sink`](https://github.com/tokio-rs/tokio-service/issues/8)
  - [Lifetimes](https://github.com/tokio-rs/tokio-service/issues/9)
    - I think we're unlikely to do anything here until we have ATC/HKT
  - [Detecting service closure](https://github.com/tokio-rs/tokio-service/issues/4)
