extern crate futures;
extern crate tokio;
extern crate tokio_executor;
extern crate tokio_timer;

#[macro_use]
mod support;
use support::*;

use futures::{
    prelude::*,
    sync::mpsc,
};
use tokio::util::StreamExt;
use tokio_timer::{
    debounce::{Debounce, Edge},
    Timer,
};

#[test]
fn debounce_leading() {
    mocked(|timer, _| {
        let (debounced, tx) = make_debounced(Edge::Leading, None);
        let items = smoke_tests(timer, tx, debounced);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0], 0);
    });
}

#[test]
fn debounce_trailing_many() {
    mocked(|timer, _| {
        let (mut debounced, tx) = make_debounced(Edge::Trailing, None);

        // Send in two items.
        tx.unbounded_send(1).unwrap();
        tx.unbounded_send(2).unwrap();

        // We shouldn't be ready yet, but we should have stored 2 as our last item.
        assert_not_ready!(debounced);

        // Go past our delay instant.
        advance(timer, ms(11));

        // Poll again, we should get 2.
        assert_ready_eq!(debounced, Some(2));

        // No more items in the stream, delay finished: we should be NotReady.
        assert_not_ready!(debounced);
    });
}

#[test]
fn debounce_trailing() {
    mocked(|timer, _| {
        let (debounced, tx) = make_debounced(Edge::Trailing, None);
        let items = smoke_tests(timer, tx, debounced);

        assert_eq!(items.len(), 1);
        assert_eq!(items[0], 4);
    });
}

#[test]
fn debounce_both() {
    mocked(|timer, _| {
        let (debounced, tx) = make_debounced(Edge::Both, None);
        let items = smoke_tests(timer, tx, debounced);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0], 0);
        assert_eq!(items[1], 4);
    });
}

#[test]
fn sample_leading() {
    mocked(|timer, _| {
        let (debounced, tx) = make_debounced(Edge::Leading, Some(3));
        let items = smoke_tests(timer, tx, debounced);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0], 0);
        assert_eq!(items[1], 3);
    });
}

#[test]
fn sample_trailing() {
    mocked(|timer, _| {
        let (debounced, tx) = make_debounced(Edge::Trailing, Some(3));
        let items = smoke_tests(timer, tx, debounced);

        assert_eq!(items.len(), 2);
        assert_eq!(items[0], 2);
        assert_eq!(items[1], 4);
    });
}

#[test]
#[should_panic]
fn sample_both_panics() {
    let (_, rx) = mpsc::unbounded::<()>();
    let _ = rx.debounce_builder()
        .max_wait(ms(10))
        .edge(Edge::Both)
        .build();
}

#[test]
fn combinator_debounce() {
    let (_, rx1) = mpsc::unbounded::<()>();
    let _ = rx1.debounce(ms(100));
}

#[test]
fn combinator_sample() {
    let (_, rx1) = mpsc::unbounded::<()>();
    let _ = rx1.sample(ms(100));
}

fn make_debounced(
    edge: Edge,
    max_wait: Option<u64>,
) -> (impl Stream<Item=u32, Error=()>, mpsc::UnboundedSender<u32>) {
    let (tx, rx) = mpsc::unbounded();
    let debounced = Debounce::new(
        rx,
        ms(10),
        edge,
        max_wait.map(ms),
    )
        .map_err(|e| panic!("stream error: {:?}", e));

    (debounced, tx)
}

fn smoke_tests(
    timer: &mut Timer<MockPark>,
    tx: mpsc::UnboundedSender<u32>,
    mut s: impl Stream<Item=u32, Error=()>,
) -> Vec<u32> {
    assert_not_ready!(s);

    let mut result = Vec::new();

    // Drive forward 1ms at a time adding items to the stream
    for i in 0..5 {
        tx.unbounded_send(i).unwrap();
        advance(timer, ms(1));

        match s.poll().unwrap() {
            Async::Ready(Some(it)) => result.push(it),
            Async::Ready(None) => break,
            Async::NotReady => {},
        }
    }

    // Pull final items out of stream
    for _ in 0..100 {
        match s.poll().unwrap() {
            Async::Ready(Some(it)) => result.push(it),
            Async::Ready(None) => break,
            Async::NotReady => {},
        }

        advance(timer, ms(1));
    }

    advance(timer, ms(1000));

    assert_not_ready!(s);
    result
}
