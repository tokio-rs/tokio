#[cfg(all(loom, test))]
macro_rules! thread_local {
    ($($tts:tt)+) => { loom::thread_local!{ $($tts)+ } }
}
