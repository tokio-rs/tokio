## Pull Requests

Pull Requests are the way concrete changes are made to the code, documentation,
and dependencies in the Tokio repository.

Even tiny pull requests (e.g., one-character pull request fixing a typo in API
documentation) are greatly appreciated. Before making a large change, it is
usually a good idea to first open an issue describing the change to solicit
feedback and guidance. This will increase the likelihood of the PR getting
merged.

### Cargo Commands

Due to the extensive use of features in Tokio, you will often need to add extra
arguments to many common cargo commands. This section lists some commonly needed
commands.

Some commands just need the `--all-features` argument:

```
cargo build --all-features
cargo check --all-features
cargo test --all-features
```

**NOTE**: there are some features that are not supported in every system, so you might
need to specify which features you want to pass to cargo (e.g., `cargo check --features=full,io-uring`)

Ideally, you should use the same version of clippy as the one used in CI
(defined by `env.rust_clippy` in [ci.yml][ci.yml]), because newer versions
might have new lints:

[ci.yml]: ../../.github/workflows/ci.yml

<!--
When updating this, also update:
- .github/workflows/ci.yml
- README.md
- tokio/README.md
- tokio/Cargo.toml
- tokio-util/Cargo.toml
- tokio-test/Cargo.toml
- tokio-stream/Cargo.toml
-->

```
cargo +1.88 clippy --all --tests --all-features
```

When building documentation, a simple `cargo doc` is not sufficient. To produce
documentation equivalent to what will be produced in docs.rs's builds of Tokio's
docs, please use:

```
RUSTDOCFLAGS="--cfg docsrs --cfg tokio_unstable" RUSTFLAGS="--cfg docsrs --cfg tokio_unstable" cargo +nightly doc --all-features [--open]
```

This turns on indicators to display the Cargo features required for
conditionally compiled APIs in Tokio, and it enables documentation of unstable
Tokio features. Notice that it is necessary to pass cfg flags to both RustDoc
*and* rustc.

There is a more concise way to build docs.rs-equivalent docs by using [`cargo
docs-rs`], which reads the above documentation flags out of Tokio's Cargo.toml
as docs.rs itself does.

[`cargo docs-rs`]: https://github.com/dtolnay/cargo-docs-rs

```
cargo install --locked cargo-docs-rs
cargo +nightly docs-rs [--open]
```

The `cargo fmt` command does not work on the Tokio codebase. You can use the
command below instead:

```
# Mac or Linux
rustfmt --check --edition 2021 $(git ls-files '*.rs')

# Powershell
Get-ChildItem . -Filter "*.rs" -Recurse | foreach { rustfmt --check --edition 2021 $_.FullName }
```
The `--check` argument prints the things that need to be fixed. If you remove
it, `rustfmt` will update your files locally instead.

You can run loom tests with
```
cd tokio # tokio crate in workspace
LOOM_MAX_PREEMPTIONS=1 LOOM_MAX_BRANCHES=10000 RUSTFLAGS="--cfg loom -C debug_assertions" \
    cargo test --lib --release --features full -- --test-threads=1 --nocapture
```
Additionally, you can also add `--cfg tokio_unstable` to the `RUSTFLAGS` environment variable to
run loom tests that test unstable features.

You can run miri tests with
```
MIRIFLAGS="-Zmiri-disable-isolation -Zmiri-strict-provenance" \
    cargo +nightly miri test --features full --lib --tests
```

### Performing spellcheck on tokio codebase

You can perform a spell-check on the Tokio codebase. For details of how to use the spellcheck tool, feel free to visit
https://github.com/drahnr/cargo-spellcheck
```
# First install the spell-check plugin
cargo install --locked cargo-spellcheck

# Then run the cargo spell check command
cargo spellcheck check
```

If the command rejects a word, you should backtick the rejected word if it's code related. If not, the
rejected word should be put into `spellcheck.dic` file.

Note that when you add a word into the file, you should also update the first line which tells the spellcheck tool
the total number of words included in the file

### Tests

If the change being proposed alters code (as opposed to only documentation for
example), it is either adding new functionality to Tokio or it is fixing
existing, broken functionality. In both of these cases, the pull request should
include one or more tests to ensure that Tokio does not regress in the future.
There are two ways to write tests: [integration tests][integration-tests]
and [documentation tests][documentation-tests].
(Tokio avoids [unit tests][unit-tests] as much as possible).

Tokio uses [conditional compilation attributes][conditional-compilation]
throughout the codebase, to modify rustc's behavior. Code marked with such
attributes can be enabled using `RUSTFLAGS` and `RUSTDOCFLAGS` environment
variables. One of the most prevalent flags passed in these variables is
the `--cfg` option. To run tests in a particular file, check first what
options #![cfg] declaration defines for that file.

For instance, to run a test marked with the 'tokio_unstable' cfg option,
you must pass this flag to the compiler when running the test.
```
$ RUSTFLAGS="--cfg tokio_unstable" cargo test -p tokio --all-features --test rt_metrics
```

#### Integration tests

Integration tests go in the same crate as the code they are testing. Each sub
crate should have a `dev-dependency` on `tokio` itself. This makes all Tokio
utilities available to use in tests, no matter the crate being tested.

The best strategy for writing a new integration test is to look at existing
integration tests in the crate and follow the style.

#### Fuzz tests

Some of our crates include a set of fuzz tests, this will be marked by a
directory `fuzz`. It is a good idea to run fuzz tests after each change.
To get started with fuzz testing you'll need to install
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz).

`cargo install --locked cargo-fuzz`

To list the available fuzzing harnesses you can run;

```bash
$ cd tokio
$ cargo fuzz list
fuzz_linked_list
```

Running a fuzz test is as simple as;

`cargo fuzz run fuzz_linked_list`

**NOTE**: Keep in mind that by default when running a fuzz test the fuzz
harness will run forever and will only exit if you `ctrl-c` or it finds
a bug.

#### Documentation tests

Ideally, every API has at least one [documentation test] that demonstrates how to
use the API. Documentation tests are run with `cargo test --doc`. This ensures
that the example is correct and provides additional test coverage.

The trick to documentation tests is striking a balance between being succinct
for a reader to understand and actually testing the API.

Same as with integration tests, when writing a documentation test, the full
`tokio` crate is available. This is especially useful for getting access to the
runtime to run the example.

The documentation tests will be visible from both the crate-specific
documentation **and** the `tokio` facade documentation via the re-export. The
example should be written from the point of view of a user that is using the
`tokio` crate. As such, the example should use the API via the facade and not by
directly referencing the crate.

The type level example for `tokio::time::timeout` provides a good example of a
documentation test:

```
/// Create a new `Timeout` set to expire in 10 milliseconds.
///
/// ```rust
/// use tokio::time::timeout;
/// use tokio::sync::oneshot;
///
/// use std::time::Duration;
///
/// # async fn dox() {
/// let (tx, rx) = oneshot::channel();
/// # tx.send(()).unwrap();
///
/// // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
/// if let Err(_) = timeout(Duration::from_millis(10), rx).await {
///     println!("did not receive value within 10 ms");
/// }
/// # }
/// ```
```

Lines that start with `/// #` are removed when the documentation is generated.

### Benchmarks

You can run benchmarks locally for the changes you've made to the tokio codebase.
Tokio currently uses [Criterion](https://github.com/bheisler/criterion.rs) as its benchmarking tool. To run a benchmark
against the changes you have made, for example, you can run;

```bash
cd benches

# Run all benchmarks.
cargo bench

# Run all tests in the `benches/fs.rs` file
cargo bench --bench fs

# Run the `async_read_buf` benchmark in `benches/fs.rs` specifically.
cargo bench async_read_buf

# After running benches, you can check the statistics under `tokio/target/criterion/`
```

You can also refer to [Criterion] docs for additional options and details.

[Criterion]: https://docs.rs/criterion/latest/criterion/

### Commits

It is a recommended best practice to keep your changes as logically grouped as
possible within individual commits. There is no limit to the number of commits
any single Pull Request may have, and many contributors find it easier to review
changes that are split across multiple commits.

That said, if you have a number of commits that are "checkpoints" and don't
represent a single logical change, please squash those together.

Note that multiple commits often get squashed when they are landed (see the
notes about [commit squashing](#commit-squashing)).

#### Commit message guidelines

A good commit message should describe what changed and why.

1. The first line should:

* contain a short description of the change (preferably 50 characters or less,
  and no more than 72 characters)
* be entirely in lowercase with the exception of proper nouns, acronyms, and
  the words that refer to code, like function/variable names
* start with an imperative verb
* not have a period at the end
* be prefixed with the name of the module being changed; usually this is the
  same as the M-* label on the PR

Examples:

* time: introduce `Timeout` and deprecate `Deadline`
* codec: export `Encoder`, `Decoder`, `Framed*`
* ci: fix the FreeBSD ci configuration

2. Keep the second line blank.
3. Wrap all other lines at 72 columns (except for long URLs).
4. If your patch fixes an open issue, you can add a reference to it at the end
   of the log. Use the `Fixes: #` prefix and the issue number. For other
   references use `Refs: #`. `Refs` may include multiple issues, separated by a
   comma.

   Examples:

    - `Fixes: #1337`
    - `Refs: #1234`

Sample complete commit message:

```txt
module: explain the commit in one line

Body of commit message is a few lines of text, explaining things
in more detail, possibly giving some background about the issue
being fixed, etc.

The body of the commit message can be several paragraphs, and
please do proper word-wrap and keep columns shorter than about
72 characters or so. That way, `git log` will show things
nicely even when it is indented.

Fixes: #1337
Refs: #453, #154
```

### Opening the Pull Request

From within GitHub, opening a new Pull Request will present you with a
[template] that should be filled out. Please try to do your best at filling out
the details, but feel free to skip parts if you're not sure what to put.

[template]: ../../.github/PULL_REQUEST_TEMPLATE.md

### Discuss and update

You will probably get feedback or requests for changes to your Pull Request.
This is a big part of the submission process so don't be discouraged! Some
contributors may sign off on the Pull Request right away, others may have
more detailed comments or feedback. This is a necessary part of the process
in order to evaluate whether the changes are correct and necessary.

**Any community member can review a PR and you might get conflicting feedback**.
Keep an eye out for comments from code owners to provide guidance on conflicting
feedback.

**Once the PR is open, do not rebase the commits**. See [Commit Squashing](#commit-squashing) for
more details.

### Commit Squashing

In most cases, **do not squash commits that you add to your Pull Request during
the review process**. When the commits in your Pull Request land, they may be
squashed into one commit per logical change. Metadata will be added to the
commit message (including links to the Pull Request, links to relevant issues,
and the names of the reviewers). The commit history of your Pull Request,
however, will stay intact on the Pull Request page.

[integration-tests]: https://doc.rust-lang.org/rust-by-example/testing/integration_testing.html
[unit-tests]: https://doc.rust-lang.org/rust-by-example/testing/unit_testing.html
[documentation-tests]: https://doc.rust-lang.org/rust-by-example/testing/doc_testing.html
[conditional-compilation]: https://doc.rust-lang.org/reference/conditional-compilation.html
