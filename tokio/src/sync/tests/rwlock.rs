use crate::sync::rwlock::*;

#[test]
fn serial_try_read_write() {
    let l = RwLock::new(42);

    {
        let g = l.try_read().unwrap();
        assert_eq!(*g, 42);

        assert!(l.try_write().is_err());

        let g2 = l.try_read().unwrap();
        assert_eq!(*g2, 42);
    }

    {
        let mut g = l.try_write().unwrap();
        assert_eq!(*g, 42);
        *g = 4242;

        assert!(l.try_read().is_err());
    }

    assert_eq!(*l.try_read().unwrap(), 4242);
}
