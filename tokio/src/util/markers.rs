/// Marker for types that are `Sync` but not `Send`
pub(crate) struct SyncNotSend(*mut ());

unsafe impl Sync for SyncNotSend {}
