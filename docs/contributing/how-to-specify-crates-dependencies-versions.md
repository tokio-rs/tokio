# How to specify crates dependencies versions

Each crate (e.g., `tokio-util`, `tokio-stream`, etc.) should specify dependencies
according to these rules:

1. The listed version should be the oldest version that the crate works with
(e.g., if `tokio-util` works with `tokio` version `1.44` but not `1.43`, then
`tokio-util` should specify version `1.44` for its `tokio` dependency).
We don't require users to use the latest version unnecessarily.
2. When a crate starts using a newer feature in a dependency, the version
should be bumped to the version that introduced it.
3. If a crate depends on an unreleased feature in a dependency, it may use
`path = ` dependency to specify this. Since path dependencies must be removed
during the release of the crate, this ensures that it can't be released until
the dependency has a new version.

Consider the following example from `tokio-stream`:

```toml
[dependencies]
futures-core = { version = "0.3.0" }
pin-project-lite = "0.2.11"
tokio = { path = "../tokio", features = ["sync"] }
```

In this case, local development of `tokio-stream` uses the local version
of `tokio` via the `path` dependency. This means that it's currently not
possible to release `tokio-stream`. Once a new version of `tokio` is
released, the path dependency will be removed from `tokio-stream`.
As mentioned before, this version should only be bumped when adding a new
feature in the crate that relies on a newer version.
