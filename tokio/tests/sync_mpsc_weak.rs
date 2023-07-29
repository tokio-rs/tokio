#![allow(clippy::redundant_clone)]
#![warn(rust_2018_idioms)]
#![cfg(feature = "sync")]

#[cfg(all(target_family = "wasm", not(target_os = "wasi")))]
use wasm_bindgen_test::wasm_bindgen_test as test;

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{Acquire, Release};
use tokio::sync::mpsc::{self, channel, unbounded_channel};
use tokio::sync::oneshot;

#[tokio::test]
async fn weak_sender() {
    let (tx, mut rx) = channel(11);

    let tx_weak = tokio::spawn(async move {
        let tx_weak = tx.clone().downgrade();

        for i in 0..10 {
            if tx.send(i).await.is_err() {
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

    let tx_weak = tx_weak.unwrap();
    let upgraded = tx_weak.upgrade();
    assert!(upgraded.is_none());
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

            let sender = self.sender.clone();

            // cannot move self.sender here
            if let Some(sender) = sender.upgrade() {
                let _ = sender.send(msg).await;
                self.sender = sender.downgrade();
            }
        }

        async fn run(&mut self) {
            let mut i = 0;
            while let Some(msg) = self.receiver.recv().await {
                self.handle_message(msg);

                if i == 0 {
                    self.send_message_to_self().await;
                }

                i += 1
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

static NUM_DROPPED: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct Msg;

impl Drop for Msg {
    fn drop(&mut self) {
        NUM_DROPPED.fetch_add(1, Release);
    }
}

// Tests that no pending messages are put onto the channel after `Rx` was
// dropped.
//
// Note: After the introduction of `WeakSender`, which internally
// used `Arc` and doesn't call a drop of the channel after the last strong
// `Sender` was dropped while more than one `WeakSender` remains, we want to
// ensure that no messages are kept in the channel, which were sent after
// the receiver was dropped.
#[tokio::test]
async fn test_msgs_dropped_on_rx_drop() {
    let (tx, mut rx) = mpsc::channel(3);

    tx.send(Msg {}).await.unwrap();
    tx.send(Msg {}).await.unwrap();

    // This msg will be pending and should be dropped when `rx` is dropped
    let sent_fut = tx.send(Msg {});

    let _ = rx.recv().await.unwrap();
    let _ = rx.recv().await.unwrap();

    sent_fut.await.unwrap();

    drop(rx);

    assert_eq!(NUM_DROPPED.load(Acquire), 3);

    // This msg will not be put onto `Tx` list anymore, since `Rx` is closed.
    assert!(tx.send(Msg {}).await.is_err());

    assert_eq!(NUM_DROPPED.load(Acquire), 4);
}

// Tests that a `WeakSender` is upgradeable when other `Sender`s exist.
#[test]
fn downgrade_upgrade_sender_success() {
    let (tx, _rx) = mpsc::channel::<i32>(1);
    let weak_tx = tx.downgrade();
    assert!(weak_tx.upgrade().is_some());
}

// Tests that a `WeakSender` fails to upgrade when no other `Sender` exists.
#[test]
fn downgrade_upgrade_sender_failure() {
    let (tx, _rx) = mpsc::channel::<i32>(1);
    let weak_tx = tx.downgrade();
    drop(tx);
    assert!(weak_tx.upgrade().is_none());
}

// Tests that a `WeakSender` cannot be upgraded after a `Sender` was dropped,
// which existed at the time of the `downgrade` call.
#[test]
fn downgrade_drop_upgrade() {
    let (tx, _rx) = mpsc::channel::<i32>(1);

    // the cloned `Tx` is dropped right away
    let weak_tx = tx.clone().downgrade();
    drop(tx);
    assert!(weak_tx.upgrade().is_none());
}

// Tests that we can upgrade a weak sender with an outstanding permit
// but no other strong senders.
#[tokio::test]
async fn downgrade_get_permit_upgrade_no_senders() {
    let (tx, _rx) = mpsc::channel::<i32>(1);
    let weak_tx = tx.downgrade();
    let _permit = tx.reserve_owned().await.unwrap();
    assert!(weak_tx.upgrade().is_some());
}

// Tests that you can downgrade and upgrade a sender with an outstanding permit
// but no other senders left.
#[tokio::test]
async fn downgrade_upgrade_get_permit_no_senders() {
    let (tx, _rx) = mpsc::channel::<i32>(1);
    let tx2 = tx.clone();
    let _permit = tx.reserve_owned().await.unwrap();
    let weak_tx = tx2.downgrade();
    drop(tx2);
    assert!(weak_tx.upgrade().is_some());
}

// Tests that `downgrade` does not change the `tx_count` of the channel.
#[test]
fn test_tx_count_weak_sender() {
    let (tx, _rx) = mpsc::channel::<i32>(1);
    let tx_weak = tx.downgrade();
    let tx_weak2 = tx.downgrade();
    drop(tx);

    assert!(tx_weak.upgrade().is_none() && tx_weak2.upgrade().is_none());
}

#[tokio::test]
async fn weak_unbounded_sender() {
    let (tx, mut rx) = unbounded_channel();

    let tx_weak = tokio::spawn(async move {
        let tx_weak = tx.clone().downgrade();

        for i in 0..10 {
            if tx.send(i).is_err() {
                return None;
            }
        }

        let tx2 = tx_weak
            .upgrade()
            .expect("expected to be able to upgrade tx_weak");
        let _ = tx2.send(20);
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

    let tx_weak = tx_weak.unwrap();
    let upgraded = tx_weak.upgrade();
    assert!(upgraded.is_none());
}

#[tokio::test]
async fn actor_weak_unbounded_sender() {
    pub struct MyActor {
        receiver: mpsc::UnboundedReceiver<ActorMessage>,
        sender: mpsc::WeakUnboundedSender<ActorMessage>,
        next_id: u32,
        pub received_self_msg: bool,
    }

    enum ActorMessage {
        GetUniqueId { respond_to: oneshot::Sender<u32> },
        SelfMessage {},
    }

    impl MyActor {
        fn new(
            receiver: mpsc::UnboundedReceiver<ActorMessage>,
            sender: mpsc::WeakUnboundedSender<ActorMessage>,
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

            let sender = self.sender.clone();

            // cannot move self.sender here
            if let Some(sender) = sender.upgrade() {
                let _ = sender.send(msg);
                self.sender = sender.downgrade();
            }
        }

        async fn run(&mut self) {
            let mut i = 0;
            while let Some(msg) = self.receiver.recv().await {
                self.handle_message(msg);

                if i == 0 {
                    self.send_message_to_self().await;
                }

                i += 1
            }

            assert!(self.received_self_msg);
        }
    }

    #[derive(Clone)]
    pub struct MyActorHandle {
        sender: mpsc::UnboundedSender<ActorMessage>,
    }

    impl MyActorHandle {
        pub fn new() -> (Self, MyActor) {
            let (sender, receiver) = mpsc::unbounded_channel();
            let actor = MyActor::new(receiver, sender.clone().downgrade());

            (Self { sender }, actor)
        }

        pub async fn get_unique_id(&self) -> u32 {
            let (send, recv) = oneshot::channel();
            let msg = ActorMessage::GetUniqueId { respond_to: send };

            // Ignore send errors. If this send fails, so does the
            // recv.await below. There's no reason to check the
            // failure twice.
            let _ = self.sender.send(msg);
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

static NUM_DROPPED_UNBOUNDED: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
struct MsgUnbounded;

impl Drop for MsgUnbounded {
    fn drop(&mut self) {
        NUM_DROPPED_UNBOUNDED.fetch_add(1, Release);
    }
}

// Tests that no pending messages are put onto the channel after `Rx` was
// dropped.
//
// Note: After the introduction of `UnboundedWeakSender`, which internally
// used `Arc` and doesn't call a drop of the channel after the last strong
// `UnboundedSender` was dropped while more than one `UnboundedWeakSender`
// remains, we want to ensure that no messages are kept in the channel, which
// were sent after the receiver was dropped.
#[tokio::test]
async fn test_msgs_dropped_on_unbounded_rx_drop() {
    let (tx, mut rx) = mpsc::unbounded_channel();

    tx.send(MsgUnbounded {}).unwrap();
    tx.send(MsgUnbounded {}).unwrap();

    // This msg will be pending and should be dropped when `rx` is dropped
    let sent = tx.send(MsgUnbounded {});

    let _ = rx.recv().await.unwrap();
    let _ = rx.recv().await.unwrap();

    sent.unwrap();

    drop(rx);

    assert_eq!(NUM_DROPPED_UNBOUNDED.load(Acquire), 3);

    // This msg will not be put onto `Tx` list anymore, since `Rx` is closed.
    assert!(tx.send(MsgUnbounded {}).is_err());

    assert_eq!(NUM_DROPPED_UNBOUNDED.load(Acquire), 4);
}

// Tests that an `WeakUnboundedSender` is upgradeable when other
// `UnboundedSender`s exist.
#[test]
fn downgrade_upgrade_unbounded_sender_success() {
    let (tx, _rx) = mpsc::unbounded_channel::<i32>();
    let weak_tx = tx.downgrade();
    assert!(weak_tx.upgrade().is_some());
}

// Tests that a `WeakUnboundedSender` fails to upgrade when no other
// `UnboundedSender` exists.
#[test]
fn downgrade_upgrade_unbounded_sender_failure() {
    let (tx, _rx) = mpsc::unbounded_channel::<i32>();
    let weak_tx = tx.downgrade();
    drop(tx);
    assert!(weak_tx.upgrade().is_none());
}

// Tests that an `WeakUnboundedSender` cannot be upgraded after an
// `UnboundedSender` was dropped, which existed at the time of the `downgrade` call.
#[test]
fn downgrade_drop_upgrade_unbounded() {
    let (tx, _rx) = mpsc::unbounded_channel::<i32>();

    // the cloned `Tx` is dropped right away
    let weak_tx = tx.clone().downgrade();
    drop(tx);
    assert!(weak_tx.upgrade().is_none());
}

// Tests that `downgrade` does not change the `tx_count` of the channel.
#[test]
fn test_tx_count_weak_unbounded_sender() {
    let (tx, _rx) = mpsc::unbounded_channel::<i32>();
    let tx_weak = tx.downgrade();
    let tx_weak2 = tx.downgrade();
    drop(tx);

    assert!(tx_weak.upgrade().is_none() && tx_weak2.upgrade().is_none());
}
