use std::marker::PhantomData;

/// `LocalRuntime`-only config options
///
/// Currently, there are no such options, but in the future, things like `!Send + !Sync` hooks may
/// be added.
#[derive(Default, Debug)]
#[non_exhaustive]
pub struct LocalOptions {
    /// Marker used to make this !Send and !Sync.
    _phantom: PhantomData<*mut u8>,
}
