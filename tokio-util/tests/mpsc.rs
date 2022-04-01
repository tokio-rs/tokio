use futures::future::poll_fn;
use tokio::sync::mpsc::{self, channel};
use tokio::sync::oneshot;
use tokio_test::task::spawn;
use tokio_test::{assert_pending, assert_ready, assert_ready_err, assert_ready_ok};
use tokio_util::sync::PollSender;

#[tokio::test]
async fn simple() {
    let (send, mut recv) = channel(3);
    let mut send = PollSender::new(send);

    for i in 1..=3i32 {
        let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
        assert_ready_ok!(reserve.poll());
        send.send_item(i).unwrap();
    }

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_pending!(reserve.poll());

    assert_eq!(recv.recv().await.unwrap(), 1);
    assert!(reserve.is_woken());
    assert_ready_ok!(reserve.poll());

    drop(recv);

    send.send_item(42).unwrap();
}

#[tokio::test]
async fn repeated_poll_reserve() {
    let (send, mut recv) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    assert_ready_ok!(reserve.poll());
    send.send_item(1).unwrap();

    assert_eq!(recv.recv().await.unwrap(), 1);
}

#[tokio::test]
async fn abort_send() {
    let (send, mut recv) = channel(3);
    let mut send = PollSender::new(send);
    let send2 = send.get_ref().cloned().unwrap();

    for i in 1..=3i32 {
        let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
        assert_ready_ok!(reserve.poll());
        send.send_item(i).unwrap();
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

    send.close();

    assert!(recv_task.is_woken());
    assert!(assert_ready!(recv_task.poll()).is_none());
}

#[tokio::test]
async fn close_sender_not_last() {
    let (send, mut recv) = channel::<i32>(3);
    let mut send = PollSender::new(send);
    let send2 = send.get_ref().cloned().unwrap();

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    send.close();

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

    send.close();

    assert!(recv_task.is_woken());
    assert!(assert_ready!(recv_task.poll()).is_none());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_err!(reserve.poll());
}

#[tokio::test]
async fn close_sender_after_pending_reserve() {
    let (send, mut recv) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    send.send_item(1).unwrap();

    assert!(recv_task.is_woken());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_pending!(reserve.poll());
    drop(reserve);

    send.close();

    assert!(send.is_closed());
    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_err!(reserve.poll());
}

#[tokio::test]
async fn close_sender_after_successful_reserve() {
    let (send, mut recv) = channel::<i32>(3);
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    drop(reserve);

    send.close();
    assert!(send.is_closed());
    assert!(!recv_task.is_woken());
    assert_pending!(recv_task.poll());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
}

#[tokio::test]
async fn abort_send_after_pending_reserve() {
    let (send, mut recv) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    send.send_item(1).unwrap();

    assert_eq!(send.get_ref().unwrap().capacity(), 0);
    assert!(!send.abort_send());

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_pending!(reserve.poll());

    assert!(send.abort_send());
    assert_eq!(send.get_ref().unwrap().capacity(), 0);
}

#[tokio::test]
async fn abort_send_after_successful_reserve() {
    let (send, mut recv) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut recv_task = spawn(recv.recv());
    assert_pending!(recv_task.poll());

    assert_eq!(send.get_ref().unwrap().capacity(), 1);
    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    assert_eq!(send.get_ref().unwrap().capacity(), 0);

    assert!(send.abort_send());
    assert_eq!(send.get_ref().unwrap().capacity(), 1);
}

#[tokio::test]
async fn closed_when_receiver_drops() {
    let (send, _) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_err!(reserve.poll());
}

#[should_panic]
#[test]
fn start_send_panics_when_idle() {
    let (send, _) = channel::<i32>(3);
    let mut send = PollSender::new(send);

    send.send_item(1).unwrap();
}

#[should_panic]
#[test]
fn start_send_panics_when_acquiring() {
    let (send, _) = channel::<i32>(1);
    let mut send = PollSender::new(send);

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_ready_ok!(reserve.poll());
    send.send_item(1).unwrap();

    let mut reserve = spawn(poll_fn(|cx| send.poll_reserve(cx)));
    assert_pending!(reserve.poll());
    send.send_item(2).unwrap();
}

#[tokio::test]
async fn weak_sender() {
    let (tx, mut rx) = channel(11);
    let tx_weak = tx.clone().downgrade();

    let tx_weak = tokio::spawn(async move {
        for i in 0..10 {
            if let Err(_) = tx.send(i).await {
                return None;
            }
        }

        let tx2 = tx_weak
            .upgrade()
            .expect("expected to be able to upgrade tx_weak");
        let _ = tx2.send(20).await;
        let tx_weak = tx2.downgrade();

        Some(tx_weak)
    })
    .await
    .unwrap();

    for i in 0..12 {
        let recvd = rx.recv().await;

        match recvd {
            Some(msg) => {
                if i == 10 {
                    assert_eq!(msg, 20);
                }
            }
            None => {
                assert_eq!(i, 11);
                break;
            }
        }
    }

    if let Some(tx_weak) = tx_weak {
        let upgraded = tx_weak.upgrade();
        assert!(upgraded.is_none());
    }
}

#[tokio::test]
async fn actor_weak_sender() {
    pub struct MyActor {
        receiver: mpsc::Receiver<ActorMessage>,
        sender: mpsc::WeakSender<ActorMessage>,
        next_id: u32,
        pub received_self_msg: bool,
    }

    enum ActorMessage {
        GetUniqueId { respond_to: oneshot::Sender<u32> },
        SelfMessage {},
    }

    impl MyActor {
        fn new(
            receiver: mpsc::Receiver<ActorMessage>,
            sender: mpsc::WeakSender<ActorMessage>,
        ) -> Self {
            MyActor {
                receiver,
                sender,
                next_id: 0,
                received_self_msg: false,
            }
        }

        fn handle_message(&mut self, msg: ActorMessage) {
            match msg {
                ActorMessage::GetUniqueId { respond_to } => {
                    self.next_id += 1;

                    // The `let _ =` ignores any errors when sending.
                    //
                    // This can happen if the `select!` macro is used
                    // to cancel waiting for the response.
                    let _ = respond_to.send(self.next_id);
                }
                ActorMessage::SelfMessage { .. } => {
                    self.received_self_msg = true;
                }
            }
        }

        async fn send_message_to_self(&mut self) {
            let msg = ActorMessage::SelfMessage {};

            if let Some(sender) = self.sender.upgrade() {
                let _ = sender.send(msg).await;
                self.sender = sender.downgrade();
            }
        }

        async fn run(&mut self) {
            let mut i = 0;
            loop {
                match self.receiver.recv().await {
                    Some(msg) => {
                        self.handle_message(msg);
                    }
                    None => {
                        break;
                    }
                }
                if i == 0 {
                    self.send_message_to_self().await;
                }
                i += 1;
            }

            assert!(self.received_self_msg);
        }
    }

    #[derive(Clone)]
    pub struct MyActorHandle {
        sender: mpsc::Sender<ActorMessage>,
    }

    impl MyActorHandle {
        pub fn new() -> (Self, MyActor) {
            let (sender, receiver) = mpsc::channel(8);
            let actor = MyActor::new(receiver, sender.clone().downgrade());

            (Self { sender }, actor)
        }

        pub async fn get_unique_id(&self) -> u32 {
            let (send, recv) = oneshot::channel();
            let msg = ActorMessage::GetUniqueId { respond_to: send };

            // Ignore send errors. If this send fails, so does the
            // recv.await below. There's no reason to check the
            // failure twice.
            let _ = self.sender.send(msg).await;
            recv.await.expect("Actor task has been killed")
        }
    }

    let (handle, mut actor) = MyActorHandle::new();

    let actor_handle = tokio::spawn(async move { actor.run().await });

    let _ = tokio::spawn(async move {
        let _ = handle.get_unique_id().await;
        drop(handle);
    })
    .await;

    let _ = actor_handle.await;
}
