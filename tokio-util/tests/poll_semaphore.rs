use std::future::Future;
use std::sync::Arc;
use std::task::Poll;
use tokio::sync::{OwnedSemaphorePermit, Semaphore};
use tokio_util::sync::PollSemaphore;

type SemRet = Option<OwnedSemaphorePermit>;

fn semaphore_poll(
    sem: &mut PollSemaphore,
) -> tokio_test::task::Spawn<impl Future<Output = SemRet> + '_> {
    let fut = futures::future::poll_fn(move |cx| sem.poll_acquire(cx));
    tokio_test::task::spawn(fut)
}

#[tokio::test]
async fn it_works() {
    let sem = Arc::new(Semaphore::new(1));
    let mut poll_sem = PollSemaphore::new(sem.clone());

    let permit = sem.acquire().await.unwrap();
    let mut poll = semaphore_poll(&mut poll_sem);
    assert!(poll.poll().is_pending());
    drop(permit);

    assert!(matches!(poll.poll(), Poll::Ready(Some(_))));
    drop(poll);

    sem.close();

    assert!(semaphore_poll(&mut poll_sem).await.is_none());

    // Check that it is fused.
    assert!(semaphore_poll(&mut poll_sem).await.is_none());
    assert!(semaphore_poll(&mut poll_sem).await.is_none());
}
