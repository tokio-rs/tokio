cfg_not_rt_and_metrics! {
    #[derive(Default)]
    pub(crate) struct TimerDriverMetrics {}

    impl TimerDriverMetrics {
        pub(crate) fn incr_entry_count(&self) {}
        pub(crate) fn dec_entry_count(&self) {}
    }
}

cfg_rt! {
    cfg_metrics! {
        pub(crate) use crate::runtime::TimerDriverMetrics;
    }
}
