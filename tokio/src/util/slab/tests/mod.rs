mod idx {
    use super::super::{page, Pack, Tid};
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn tid_roundtrips(tid in 0usize..Tid::BITS) {
            let tid = Tid::from_usize(tid);
            let packed = tid.pack(0);
            assert_eq!(tid, Tid::from_packed(packed));
        }

        #[test]
        fn idx_roundtrips(
            tid in 0usize..Tid::BITS,
            addr in 0usize..page::Addr::BITS,
        ) {
            let tid = Tid::from_usize(tid);
            let addr = page::Addr::from_usize(addr);
            let packed = tid.pack(addr.pack(0));
            assert_eq!(addr, page::Addr::from_packed(packed));
            assert_eq!(tid, Tid::from_packed(packed));
        }
    }
}

#[cfg(loom)]
mod loom;
#[cfg(loom)]
pub(super) use self::loom::test_util;
