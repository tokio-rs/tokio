use futures::future::poll_fn;
use tokio::sync::mpsc::channel;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok};
use tokio_util::sync::PollSender;

#[tokio::test]
async fn test_simple() {
    let (send, mut recv) = channel(3);
    let mut send = PollSender::new(send);

    for i in 1..=3i32 {
        send.start_send(i).unwrap();
        assert_ready_ok!(spawn(poll_fn(|cx| send.poll_send_done(cx))).poll());
    }

    send.start_send(4).unwrap();
    let mut fourth_send = spawn(poll_fn(|cx| send.poll_send_done(cx)));
    assert_pending!(fourth_send.poll());
    assert_eq!(recv.recv().await.unwrap(), 1);
    assert!(fourth_send.is_woken());
    assert_ready_ok!(fourth_send.poll());

    drop(recv);

    // Here, start_send is not guaranteed to fail, but if it doesn't the first
    // call to poll_send_done should.
    if send.start_send(5).is_ok() {
        assert_ready_err!(spawn(poll_fn(|cx| send.poll_send_done(cx))).poll());
    }
}

#[tokio::test]
async fn test_abort() {
    let (send, mut recv) = channel(3);
    let mut send = PollSender::new(send);
    let send2 = send.clone_inner().unwrap();

    for i in 1..=3i32 {
        send.start_send(i).unwrap();
        assert_ready_ok!(spawn(poll_fn(|cx| send.poll_send_done(cx))).poll());
    }

    send.start_send(4).unwrap();
    {
        let mut fourth_send = spawn(poll_fn(|cx| send.poll_send_done(cx)));
        assert_pending!(fourth_send.poll());
        assert_eq!(recv.recv().await.unwrap(), 1);
        assert!(fourth_send.is_woken());
    }

    let mut send2_send = spawn(send2.send(5));
    assert_pending!(send2_send.poll());
    send.abort_send();
    assert!(send2_send.is_woken());
    assert_ready_ok!(send2_send.poll());

    assert_eq!(recv.recv().await.unwrap(), 2);
    assert_eq!(recv.recv().await.unwrap(), 3);
    assert_eq!(recv.recv().await.unwrap(), 5);
}

#[tokio::test]
async fn close_sender_last() {
    let (send, mut recv) = channel::<i32>(3);
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    send.close_this_sender();

    assert!(recv_task.is_woken());
    assert!(assert_ready!(recv_task.poll()).is_none());
}

#[tokio::test]
async fn close_sender_not_last() {
    let (send, mut recv) = channel::<i32>(3);
    let send2 = send.clone();
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    send.close_this_sender();

    assert!(!recv_task.is_woken());
    assert_pending!(recv_task.poll());

    drop(send2);

    assert!(recv_task.is_woken());
    assert!(assert_ready!(recv_task.poll()).is_none());
}
