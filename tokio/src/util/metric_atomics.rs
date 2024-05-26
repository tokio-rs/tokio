use std::sync::atomic::Ordering;

cfg_64bit_metrics! {
    use std::sync::atomic::AtomicU64;
}

/// `AtomicU64` that is is a no-op on platforms without 64-bit atomics
///
/// When used on platforms without 64-bit atomics, writes to this are no-ops.
/// The `load` method is only defined when 64-bit atomics are available.
#[derive(Debug, Default)]
pub(crate) struct MetricAtomicU64 {
    #[cfg(target_has_atomic = "64")]
    value: AtomicU64,
}

// some of these are currently only used behind cfg_unstable
#[allow(dead_code)]
impl MetricAtomicU64 {
    // Load is only defined when supported
    cfg_64bit_metrics! {
        pub(crate) fn load(&self, ordering: Ordering) -> u64 {
            self.value.load(ordering)
        }
    }

    cfg_64bit_metrics! {
        pub(crate) fn store(&self, val: u64, ordering: Ordering) {
            self.value.store(val, ordering)
        }

        pub(crate) fn new(value: u64) -> Self {
            Self { value: AtomicU64::new(value) }
        }

        pub(crate) fn add(&self, value: u64, ordering: Ordering) {
            self.value.fetch_add(value, ordering);
        }
    }

    cfg_no_64bit_metrics! {
        pub(crate) fn store(&self, _val: u64, _ordering: Ordering) { }
        // on platforms without 64-bit atomics, fetch-add returns unit
        pub(crate) fn add(&self, _value: u64, _ordering: Ordering) {  }
        pub(crate) fn new(_value: u64) -> Self { Self { } }
    }
}
