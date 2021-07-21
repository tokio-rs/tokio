use crate::runtime::tests::loom_oneshot as oneshot;
use crate::runtime::Builder;
use crate::task::LocalSet;

use std::task::Poll;

#[test]
fn wake_during_shutdown() {
    loom::model(|| {
        let rt = Builder::new_current_thread().build().unwrap();
        let ls = LocalSet::new();

        let (send, recv) = oneshot::channel();

        ls.spawn_local(async move {
            let mut send = Some(send);

            let () = futures::future::poll_fn(|cx| {
                if let Some(send) = send.take() {
                    send.send(cx.waker().clone());
                }

                Poll::Pending
            })
            .await;
        });

        let handle = loom::thread::spawn(move || {
            let waker = recv.recv();
            waker.wake();
        });

        ls.block_on(&rt, crate::task::yield_now());

        drop(ls);
        handle.join().unwrap();
        drop(rt);
    });
}
