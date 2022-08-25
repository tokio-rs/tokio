pub(crate) mod current_thread;
pub(crate) use current_thread::CurrentThread;

cfg_rt_multi_thread! {
    pub(crate) mod multi_thread;
    pub(crate) use multi_thread::MultiThread;
}
