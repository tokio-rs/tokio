# Contributing to Tokio

:balloon: Thanks for your help improving the project! We are so happy to have
you!

There are opportunities to contribute to Tokio at any level. It doesn't matter if
you are just getting started with Rust or are the most weathered expert, we can
use your help.

**No contribution is too small and all contributions are valued.**

This guide will help you get started. **Do not let this guide intimidate you**.
It should be considered a map to help you navigate the process.

The [dev channel][dev] is available for any concerns not covered in this guide, please join
us!

[dev]: https://discord.gg/tokio

## Conduct

The Tokio project adheres to the [Rust Code of Conduct][coc]. This describes
the _minimum_ behavior expected from all contributors. Instances of violations of the
Code of Conduct can be reported by contacting the project team at
[moderation@tokio.rs](mailto:moderation@tokio.rs).

[coc]: https://github.com/rust-lang/rust/blob/master/CODE_OF_CONDUCT.md

## Contributing in Issues

For any issue, there are fundamentally three ways an individual can contribute:

1. By opening the issue for discussion: For instance, if you believe that you
   have discovered a bug in Tokio, creating a new issue in [the tokio-rs/tokio
   issue tracker][issue] is the way to report it.

2. By helping to triage the issue: This can be done by providing
   supporting details (a test case that demonstrates a bug), providing
   suggestions on how to address the issue, or ensuring that the issue is tagged
   correctly.

3. By helping to resolve the issue: Typically this is done either in the form of
   demonstrating that the issue reported is not a problem after all, or more
   often, by opening a Pull Request that changes some bit of something in
   Tokio in a concrete and reviewable manner.

[issue]: https://github.com/tokio-rs/tokio/issues

**Anybody can participate in any stage of contribution**. We urge you to
participate in the discussion around bugs and participate in reviewing PRs.

### Asking for General Help

If you have reviewed existing documentation and still have questions or are
having problems, you can [open a discussion] asking for help.

In exchange for receiving help, we ask that you contribute back a documentation
PR that helps others avoid the problems that you encountered.

[open a discussion]: https://github.com/tokio-rs/tokio/discussions/new

### Submitting a Bug Report

When opening a new issue in the Tokio issue tracker, you will be presented
with a basic template that should be filled in. If you believe that you have
uncovered a bug, please fill out this form, following the template to the best
of your ability. Do not worry if you cannot answer every detail, just fill in
what you can.

The two most important pieces of information we need in order to properly
evaluate the report is a description of the behavior you are seeing and a simple
test case we can use to recreate the problem on our own. If we cannot recreate
the issue, it becomes impossible for us to fix.

In order to rule out the possibility of bugs introduced by userland code, test
cases should be limited, as much as possible, to using only Tokio APIs.

See [How to create a Minimal, Complete, and Verifiable example][mcve].

[mcve]: https://stackoverflow.com/help/mcve

### Triaging a Bug Report

Once an issue has been opened, it is not uncommon for there to be discussion
around it. Some contributors may have differing opinions about the issue,
including whether the behavior being seen is a bug or a feature. This discussion
is part of the process and should be kept focused, helpful, and professional.

Short, clipped responses—that provide neither additional context nor supporting
detail—are not helpful or professional. To many, such responses are simply
annoying and unfriendly.

Contributors are encouraged to help one another make forward progress as much as
possible, empowering one another to solve issues collaboratively. If you choose
to comment on an issue that you feel either is not a problem that needs to be
fixed, or if you encounter information in an issue that you feel is incorrect,
explain why you feel that way with additional supporting context, and be willing
to be convinced that you may be wrong. By doing so, we can often reach the
correct outcome much faster.

### Resolving a Bug Report

In the majority of cases, issues are resolved by opening a Pull Request. The
process for opening and reviewing a Pull Request is similar to that of opening
and triaging issues, but carries with it a necessary review and approval
workflow that ensures that the proposed changes meet the minimal quality and
functional guidelines of the Tokio project.

## Pull Requests

Pull Requests are the way concrete changes are made to the code, documentation,
and dependencies in the Tokio repository.

Even tiny pull requests (e.g., one character pull request fixing a typo in API
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

Ideally, you should use the same version of clippy as the one used in CI
(defined by `env.rust_clippy` in [ci.yml][ci.yml]), because newer versions
might have new lints:

[ci.yml]: .github/workflows/ci.yml

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

You can perform spell-check on tokio codebase. For details of how to use the spellcheck tool, feel free to visit
https://github.com/drahnr/cargo-spellcheck
```
# First install the spell-check plugin
cargo install --locked cargo-spellcheck

# Then run the cargo spell check command
cargo spellcheck check
```

if the command rejects a word, you should backtick the rejected word if it's code related. If not, the
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
attributes can be enabled using RUSTFLAGS and RUSTDOCFLAGS environment
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

The documentation tests will be visible from both the crate specific
documentation **and** the `tokio` facade documentation via the re-export. The
example should be written from the point of view of a user that is using the
`tokio` crate. As such, the example should use the API via the facade and not by
directly referencing the crate.

The type level example for `tokio_timer::Timeout` provides a good example of a
documentation test:

```
/// // import the `timeout` function, usually this is done
/// // with `use tokio::prelude::*`
/// use tokio::prelude::FutureExt;
/// use futures::Stream;
/// use futures::sync::mpsc;
/// use std::time::Duration;
///
/// # fn main() {
/// let (tx, rx) = mpsc::unbounded();
/// # tx.unbounded_send(()).unwrap();
/// # drop(tx);
///
/// let process = rx.for_each(|item| {
///     // do something with `item`
/// # drop(item);
/// # Ok(())
/// });
///
/// # tokio::runtime::current_thread::block_on_all(
/// // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
/// process.timeout(Duration::from_millis(10))
/// # ).unwrap();
/// # }
```

Given that this is a *type* level documentation test and the primary way users
of `tokio` will create an instance of `Timeout` is by using
`FutureExt::timeout`, this is how the documentation test is structured.

Lines that start with `/// #` are removed when the documentation is generated.
They are only there to get the test to run. The `block_on_all` function is the
easiest way to execute a future from a test.

If this were a documentation test for the `Timeout::new` function, then the
example would explicitly use `Timeout::new`. For example:

```
/// use tokio::timer::Timeout;
/// use futures::Future;
/// use futures::sync::oneshot;
/// use std::time::Duration;
///
/// # fn main() {
/// let (tx, rx) = oneshot::channel();
/// # tx.send(()).unwrap();
///
/// # tokio::runtime::current_thread::block_on_all(
/// // Wrap the future with a `Timeout` set to expire in 10 milliseconds.
/// Timeout::new(rx, Duration::from_millis(10))
/// # ).unwrap();
/// # }
```

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

You can also refer to Criterion docs for additional options and details.

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

[template]: .github/PULL_REQUEST_TEMPLATE.md

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

## Reviewing Pull Requests

**Any Tokio community member is welcome to review any pull request**.

All Tokio contributors who choose to review and provide feedback on Pull
Requests have a responsibility to both the project and the individual making the
contribution. Reviews and feedback must be helpful, insightful, and geared
towards improving the contribution as opposed to simply blocking it. If there
are reasons why you feel the PR should not land, explain what those are. Do not
expect to be able to block a Pull Request from advancing simply because you say
"No" without giving an explanation. Be open to having your mind changed. Be open
to working with the contributor to make the Pull Request better.

Reviews that are dismissive or disrespectful of the contributor or any other
reviewers are strictly counter to the Code of Conduct.

When reviewing a Pull Request, the primary goals are for the codebase to improve
and for the person submitting the request to succeed. **Even if a Pull Request
does not land, the submitters should come away from the experience feeling like
their effort was not wasted or unappreciated**. Every Pull Request from a new
contributor is an opportunity to grow the community.

### Review a bit at a time.

Do not overwhelm new contributors.

It is tempting to micro-optimize and make everything about relative performance,
perfect grammar, or exact style matches. Do not succumb to that temptation.

Focus first on the most significant aspects of the change:

1. Does this change make sense for Tokio?
2. Does this change make Tokio better, even if only incrementally?
3. Are there clear bugs or larger scale issues that need attending to?
4. Is the commit message readable and correct? If it contains a breaking change
   is it clear enough?

Note that only **incremental** improvement is needed to land a PR. This means
that the PR does not need to be perfect, only better than the status quo. Follow
up PRs may be opened to continue iterating.

When changes are necessary, *request* them, do not *demand* them, and **do not
assume that the submitter already knows how to add a test or run a benchmark**.

Specific performance optimization techniques, coding styles and conventions
change over time. The first impression you give to a new contributor never does.

Nits (requests for small changes that are not essential) are fine, but try to
avoid stalling the Pull Request. Most nits can typically be fixed by the Tokio
Collaborator landing the Pull Request but they can also be an opportunity for
the contributor to learn a bit more about the project.

It is always good to clearly indicate nits when you comment: e.g.
`Nit: change foo() to bar(). But this is not blocking.`

If your comments were addressed but were not folded automatically after new
commits or if they proved to be mistaken, please, [hide them][hiding-a-comment]
with the appropriate reason to keep the conversation flow concise and relevant.

### Be aware of the person behind the code

Be aware that *how* you communicate requests and reviews in your feedback can
have a significant impact on the success of the Pull Request. Yes, we may land
a particular change that makes Tokio better, but the individual might just not
want to have anything to do with Tokio ever again. The goal is not just having
good code.

### Abandoned or Stalled Pull Requests

If a Pull Request appears to be abandoned or stalled, it is polite to first
check with the contributor to see if they intend to continue the work before
checking if they would mind if you took it over (especially if it just has nits
left). When doing so, it is courteous to give the original contributor credit
for the work they started (either by preserving their name and email address in
the commit log, or by using an `Author: ` meta-data tag in the commit.

_Adapted from the [Node.js contributing guide][node]_.

[node]: https://github.com/nodejs/node/blob/master/CONTRIBUTING.md
[hiding-a-comment]: https://help.github.com/articles/managing-disruptive-comments/#hiding-a-comment
[documentation test]: https://doc.rust-lang.org/rustdoc/documentation-tests.html

## Keeping track of issues and PRs

The Tokio GitHub repository has a lot of issues and PRs to keep track of. This
section explains the meaning of various labels, as well as our [GitHub
project][project]. The section is primarily targeted at maintainers.  Most
contributors aren't able to set these labels.

### Area

The area label describes the crates relevant to this issue or PR.

 - **A-tokio** This issue concerns the main Tokio crate.
 - **A-tokio-util** This issue concerns the `tokio-util` crate.
 - **A-tokio-tls** This issue concerns the `tokio-tls` crate. Only used for
   older issues, as the crate has been moved to another repository.
 - **A-tokio-test** The issue concerns the `tokio-test` crate.
 - **A-tokio-macros** This issue concerns the `tokio-macros` crate. Should only
   be used for the procedural macros, and not `join!` or `select!`.
 - **A-ci** This issue concerns our GitHub Actions setup.

### Category

 - **C-bug** This is a bug-report. Bug-fix PRs use `C-enhancement` instead.
 - **C-enhancement** This is a PR that adds a new features.
 - **C-maintenance** This is an issue or PR about stuff such as documentation,
   GitHub Actions or code quality.
 - **C-feature-request** This is a feature request. Implementations of feature
   requests use `C-enhancement` instead.
 - **C-feature-accepted** If you submit a PR for this feature request, we wont
   close it with the reason "we don't want this". Issues with this label should
   also have the `C-feature-request` label.
 - **C-musing** Stuff like tracking issues or roadmaps. "musings about a better
   world"
 - **C-proposal** A proposal of some kind, and a request for comments.
 - **C-question** A user question. Large overlap with GitHub discussions.
 - **C-request** A non-feature request, e.g. "please add deprecation notices to
   `-alpha.*` versions of crates"

### Calls for participation

 - **E-help-wanted** Stuff where we want help. Often seen together with `C-bug`
   or `C-feature-accepted`.
 - **E-easy** This is easy, ranging from quick documentation fixes to stuff you
   can do after reading the tutorial on our website.
 - **E-medium** This is not `E-easy` or `E-hard`.
 - **E-hard** This either involves very tricky code, is something we don't know
   how to solve, or is difficult for some other reason.
 - **E-needs-mvce** This bug is missing a minimal complete and verifiable
   example.

The "E-" prefix is the same as used in the Rust compiler repository. Some
issues are missing a difficulty rating, but feel free to ask on our Discord
server if you want to know how difficult an issue likely is.

### Module

The module label provides a more fine grained categorization than **Area**.

 - **M-blocking** Things relevant to `spawn_blocking`, `block_in_place`.
 - **M-codec** The `tokio_util::codec` module.
 - **M-compat** The `tokio_util::compat` module.
 - **M-coop** Things relevant to coop.
 - **M-fs** The `tokio::fs` module.
 - **M-io** The `tokio::io` module.
 - **M-macros** Issues about any kind of macro.
 - **M-net** The `tokio::net` module.
 - **M-process** The `tokio::process` module.
 - **M-runtime** The `tokio::runtime` module.
 - **M-signal** The `tokio::signal` module.
 - **M-sync** The `tokio::sync` module.
 - **M-task** The `tokio::task` module.
 - **M-time** The `tokio::time` module.
 - **M-tracing** Tracing support in Tokio.

### Topic

Some extra information.

 - **T-docs** This is about documentation.
 - **T-performance** This is about performance.
 - **T-v0.1.x** This is about old Tokio.

Any label not listed here is not in active use.

[project]: https://github.com/orgs/tokio-rs/projects/1

## LTS guarantees

Tokio ≥1.0.0 comes with LTS guarantees:

 * A minimum of 5 years of maintenance.
 * A minimum of 3 years before a hypothetical 2.0 release.

The goal of these guarantees is to provide stability to the ecosystem.

## Minimum Supported Rust Version (MSRV)

 * All Tokio ≥1.0.0 releases will support at least a 6-month old Rust
   compiler release.
 * The MSRV will only be increased on 1.x releases.

## Versioning Policy

With Tokio ≥1.0.0:

 * Patch (1.\_.x) releases _should only_ contain bug fixes or documentation
   changes. Besides this, these releases should not substantially change
   runtime behavior.
 * Minor (1.x) releases may contain new functionality, MSRV increases (see
   above), minor dependency updates, deprecations, and larger internal
   implementation changes.

This is as defined by [Semantic Versioning 2.0](https://semver.org/).

## Releasing

Since the Tokio project consists of a number of crates, many of which depend on
each other, releasing new versions to crates.io can involve some complexities.
When releasing a new version of a crate, follow these steps:

1. **Ensure that the release crate has no path dependencies.** When the HEAD
   version of a Tokio crate requires unreleased changes in another Tokio crate,
   the crates.io dependency on the second crate will be replaced with a path
   dependency. Crates with path dependencies cannot be published, so before
   publishing the dependent crate, any path dependencies must also be published.
   This should be done through a form of depth-first tree traversal:

   1. Starting with the first path dependency in the crate to be released,
      inspect the `Cargo.toml` for the dependency. If the dependency has any
      path dependencies of its own, repeat this step with the first such
      dependency.
   2. Begin the release process for the path dependency.
   3. Once the path dependency has been published to crates.io, update the
      dependent crate to depend on the crates.io version.
   4. When all path dependencies have been published, the dependent crate may
      be published.

   To verify that a crate is ready to publish, run:

   ```bash
   bin/publish --dry-run <CRATE NAME> <CRATE VERSION>
   ```

2. **Update Cargo metadata.** After releasing any path dependencies, update the
   `version` field in `Cargo.toml` to the new version, and the `documentation`
   field to the docs.rs URL of the new version.
3. **Update other documentation links.** Update the "Documentation" link in the
   crate's `README.md` to point to the docs.rs URL of the new version.
4. **Update the changelog for the crate.** Each crate in the Tokio repository
   has its own `CHANGELOG.md` in that crate's subdirectory. Any changes to that
   crate since the last release should be added to the changelog. Change
   descriptions may be taken from the Git history, but should be edited to
   ensure a consistent format, based on [Keep A Changelog][keep-a-changelog].
   Other entries in that crate's changelog may also be used for reference.
5. **Perform a final audit for breaking changes.** Compare the HEAD version of
   crate with the Git tag for the most recent release version. If there are any
   breaking API changes, determine if those changes can be made without breaking
   existing APIs. If so, resolve those issues. Otherwise, if it is necessary to
   make a breaking release, update the version numbers to reflect this.
6. **Open a pull request with your changes.** Once that pull request has been
   approved by a maintainer and the pull request has been merged, continue to
   the next step.
7. **Release the crate.** Run the following command:

   ```bash
   bin/publish <NAME OF CRATE> <VERSION>
   ```

   Your editor and prompt you to edit a message for the tag. Copy the changelog
   entry for that release version into your editor and close the window.

[keep-a-changelog]: https://github.com/olivierlacan/keep-a-changelog/blob/master/CHANGELOG.md
[unit-tests]: https://doc.rust-lang.org/rust-by-example/testing/unit_testing.html
[integration-tests]: https://doc.rust-lang.org/rust-by-example/testing/integration_testing.html
[documentation-tests]: https://doc.rust-lang.org/rust-by-example/testing/doc_testing.html
[conditional-compilation]: https://doc.rust-lang.org/reference/conditional-compilation.html
