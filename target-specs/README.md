This is used for the `no-atomic-u64-test` ci check that verifies that Tokio
works even if the `AtomicU64` type is missing.

When increasing the nightly compiler version, you may need to regenerate this
target using the following command:
```
rustc +nightly -Z unstable-options --print target-spec-json --target i686-unknown-linux-gnu | grep -v 'is-builtin' | sed 's/"max-atomic-width": 64/"max-atomic-width": 32/' > target-specs/i686-unknown-linux-gnu.json
```

