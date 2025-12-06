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

   Your editor will prompt you to edit a message for the tag. Copy the changelog
   entry for that release version into your editor and close the window.

[keep-a-changelog]: https://github.com/olivierlacan/keep-a-changelog/blob/master/CHANGELOG.md
