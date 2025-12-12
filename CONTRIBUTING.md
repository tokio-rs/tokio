# Contributing to Tokio

Thanks for your help improving Tokio! We are so happy to have you!

There are opportunities to contribute to Tokio at any level. It doesn't matter if
you are just getting started with Rust or are the most weathered expert, we can
use your help.

**No contribution is too small and all contributions are valued.**

See the [contributing guidelines] to get started.

[contributing guidelines]: docs/contributing/README.md

## Code of Conduct

The Tokio project adheres to the [Rust Code of Conduct][coc]. This describes
the _minimum_ behavior expected from all contributors. Instances of violations of the
Code of Conduct can be reported by contacting the project team at
[moderation@tokio.rs](mailto:moderation@tokio.rs).

[coc]: https://github.com/rust-lang/rust/blob/master/CODE_OF_CONDUCT.md

## Need Help?

Reach out to us on the [Discord server] for any concern not covered in this guide.

[Discord server]: https://discord.gg/tokio

## LTS guarantees

In Tokio ≥1.0.0, each LTS release comes with the guarantee of at least one year of
backported fixes.

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
