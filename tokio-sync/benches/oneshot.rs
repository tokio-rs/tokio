#![feature(test)]

extern crate tokio_sync;
extern crate futures;
extern crate test;

mod tokio {
    use futures::{future, Async, Future};
    use tokio_sync::oneshot;
    use test::{self, Bencher};

    #[bench]
    fn same_thread_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            tx.send(1);

            assert_eq!(Async::Ready(1), rx.poll().unwrap());
        });
    }

    #[bench]
    fn same_thread_recv_multi_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            future::lazy(|| {
                rx.poll();
                rx.poll();
                rx.poll();
                rx.poll();

                tx.send(1);
                assert_eq!(Async::Ready(1), rx.poll().unwrap());

                Ok::<_, ()>(())
            }).wait().unwrap();
        });
    }
}

mod legacy {
    use futures::{future, Async, Future};
    use futures::sync::oneshot;
    use test::{self, Bencher};

    #[bench]
    fn same_thread_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            tx.send(1);

            assert_eq!(Async::Ready(1), rx.poll().unwrap());
        });
    }

    #[bench]
    fn same_thread_recv_multi_send_recv(b: &mut Bencher) {
        b.iter(|| {
            let (tx, mut rx) = oneshot::channel();

            future::lazy(|| {
                rx.poll();
                rx.poll();
                rx.poll();
                rx.poll();

                tx.send(1);
                assert_eq!(Async::Ready(1), rx.poll().unwrap());

                Ok::<_, ()>(())
            }).wait().unwrap();
        });
    }
}
