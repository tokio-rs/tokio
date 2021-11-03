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
        let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
        assert_ready_ok!(reserve.poll());
        send.start_send(i).unwrap();
    }

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_pending!(reserve.poll());

    assert_eq!(recv.recv().await.unwrap(), 1);
    assert!(reserve.is_woken());
    assert_ready_ok!(reserve.poll());

    drop(recv);

    send.start_send(42).unwrap();
}

#[tokio::test]
async fn test_repeated_poll_reserve() {
    let (send, mut recv) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    assert_ready_ok!(reserve.poll());
    send.start_send(1).unwrap();

    assert_eq!(recv.recv().await.unwrap(), 1);
}

#[tokio::test]
async fn test_abort() {
    let (send, mut recv) = channel(3);
    let mut send = PollSender::new(send);
    let send2 = send.clone_inner().unwrap();

    for i in 1..=3i32 {
        let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
        assert_ready_ok!(reserve.poll());
        send.start_send(i).unwrap();
    }

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_pending!(reserve.poll());
    assert_eq!(recv.recv().await.unwrap(), 1);
    assert!(reserve.is_woken());
    assert_ready_ok!(reserve.poll());

    let mut send2_send = spawn(send2.send(5));
    assert_pending!(send2_send.poll());
    assert!(send.abort_send());
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

#[tokio::test]
async fn close_sender_before_reserve() {
    let (send, mut recv) = channel::<i32>(3);
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    send.close_this_sender();

    assert!(recv_task.is_woken());
    assert!(assert_ready!(recv_task.poll()).is_none());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_err!(reserve.poll());
}

#[tokio::test]
async fn close_sender_after_reserve() {
    let (send, mut recv) = channel::<i32>(3);
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());

    send.close_this_sender();

    assert!(!recv_task.is_woken());
    assert_pending!(recv_task.poll());
    assert!(send.abort_send());

    assert!(recv_task.is_woken());
    assert!(assert_ready!(recv_task.poll()).is_none());

    let result = send.start_send(42);
    assert!(result.is_err());

    let inner = result.unwrap_err().into_inner();
    assert_eq!(inner, Some(42));
}

#[should_panic]
#[test]
fn start_send_panics_when_idle() {
    let (send, _) = channel::<i32>(3);
    let mut send = PollSender::new(send);

    send.start_send(1).unwrap();
}

#[should_panic]
#[test]
fn start_send_panics_when_acquiring() {
    let (send, _) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    send.start_send(1).unwrap();

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_pending!(reserve.poll());
    send.start_send(2).unwrap();
}
