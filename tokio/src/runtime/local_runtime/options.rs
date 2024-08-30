/// `LocalRuntime`-only config options
///
/// Currently, there are no such options, but in the future, things like `!Send + !Sync` hooks may
/// be added.
#[derive(Default, Debug)]
#[non_exhaustive]
pub struct LocalOptions {
    // todo add local hooks at a later point
}
