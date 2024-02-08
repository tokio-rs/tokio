/// Marker for types that are `Sync` but not `Send`
#[allow(dead_code)]
pub(crate) struct SyncNotSend(*mut ());

unsafe impl Sync for SyncNotSend {}

cfg_rt! {
    #[allow(dead_code)]
    pub(crate) struct NotSendOrSync(*mut ());
}
