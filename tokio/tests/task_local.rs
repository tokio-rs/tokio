#![cfg(feature = "full")]

tokio::task_local! {
    static REQ_ID: u32;
    pub static FOO: bool;
}

#[tokio::test(flavor = "multi_thread")]
async fn local() {
    let j1 = tokio::spawn(REQ_ID.scope(1, async move {
        assert_eq!(REQ_ID.get(), 1);
        assert_eq!(REQ_ID.get(), 1);
    }));

    let j2 = tokio::spawn(REQ_ID.scope(2, async move {
        REQ_ID.with(|v| {
            assert_eq!(REQ_ID.get(), 2);
            assert_eq!(*v, 2);
        });

        tokio::time::sleep(std::time::Duration::from_millis(10)).await;

        assert_eq!(REQ_ID.get(), 2);
    }));

    let j3 = tokio::spawn(FOO.scope(true, async move {
        assert!(FOO.get());
    }));

    j1.await.unwrap();
    j2.await.unwrap();
    j3.await.unwrap();
}

tokio::task_local! {
    static KEY: u32;
}

struct Guard(u32);
impl Drop for Guard {
    fn drop(&mut self) {
        assert_eq!(KEY.try_with(|x| *x), Ok(self.0));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn task_local_available_on_drop() {
    let (tx, rx) = tokio::sync::oneshot::channel();

    let h = tokio::spawn(KEY.scope(42, async move {
        let _g = Guard(42);
        let _ = tx.send(());
        std::future::pending::<()>().await;
    }));

    rx.await.unwrap();

    h.abort();

    let err = h.await.unwrap_err();
    if err.is_panic() {
        panic!("{}", err);
    }
}
