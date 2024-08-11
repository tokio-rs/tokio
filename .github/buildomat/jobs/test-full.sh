#!/bin/bash
#:
#: name = "illumos-test-full"
#: variety = "basic"
#: target = "omnios-r151038"
#: rust_toolchain = "stable"

# basic illumos test job with --features full.
# TODO(eliza): consider splitting the "build" and "test" jobs into separate
# buildomat jobs, so that the build and test jobs can fail independently.

set -o errexit
set -o pipefail
set -o xtrace


# These are the same env vars that are set for all GitHub Actions CI jobs.
export RUSTFLAGS="-Dwarnings"
export RUST_BACKTRACE=1
# We're building once, so there's no need to incur the overhead of an
# incremental build.
export CARGO_INCREMENTAL=0

# NOTE: Currently we use the latest cargo-nextest release, since this is what
# the Linux CI jobs do. If we ever start pinning our nextest version, this
# should be changed to match that.
NEXTEST_VERSION='latest'


curl -sSfL --retry 10 "https://get.nexte.st/$NEXTEST_VERSION/illumos" | gunzip | tar -xvf - -C ~/.cargo/bin

# Print the current test execution environment
uname -a
cargo --version
rustc --version

banner build
ptime -m cargo test --no-run --all --verbose --features full

banner tests
ptime -m cargo nextest run --features full

banner doctests
ptime -m cargo test --doc --verbose --features full
