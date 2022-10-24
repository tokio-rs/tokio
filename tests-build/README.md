Tests the various combination of feature flags. This is broken out to a separate
crate to work around limitations with cargo features.

To run all of the tests in this directory, run the following commands:
```
cargo test --features full
cargo test --features rt
```
If one of the tests fail, you can pass `TRYBUILD=overwrite` to the `cargo test`
command that failed to have it regenerate the test output.
