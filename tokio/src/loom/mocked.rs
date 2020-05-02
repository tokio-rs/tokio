pub(crate) use loom::*;

pub(crate) mod rand {
    pub(crate) fn seed() -> u64 {
        1
    }
}

pub(crate) mod sys {
    pub(crate) fn num_cpus() -> usize {
        2
    }
}
