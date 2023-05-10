#![no_main]

use libfuzzer_sys::fuzz_target;
use std::pin::Pin;

use tokio_stream::{self as stream, Stream, StreamMap};
use tokio_test::{assert_pending, assert_ready, task};

macro_rules! assert_ready_none {
    ($($t:tt)*) => {
        match assert_ready!($($t)*) {
            None => {}
            Some(v) => panic!("expected `None`, got `Some({:?})`", v),
        }
    };
}

fn pin_box<T: Stream<Item = U> + 'static, U>(s: T) -> Pin<Box<dyn Stream<Item = U>>> {
    Box::pin(s)
}

fuzz_target!(|data: [bool; 64]| {
    use std::task::{Context, Poll};

    struct DidPoll<T> {
        did_poll: bool,
        inner: T,
    }

    impl<T: Stream + Unpin> Stream for DidPoll<T> {
        type Item = T::Item;

        fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<T::Item>> {
            self.did_poll = true;
            Pin::new(&mut self.inner).poll_next(cx)
        }
    }

    // Try the test with each possible length.
    for len in 0..data.len() {
        let mut map = task::spawn(StreamMap::new());
        let mut expect = 0;

        for (i, is_empty) in data[..len].iter().copied().enumerate() {
            let inner = if is_empty {
                pin_box(stream::empty::<()>())
            } else {
                expect += 1;
                pin_box(stream::pending::<()>())
            };

            let stream = DidPoll {
                did_poll: false,
                inner,
            };

            map.insert(i, stream);
        }

        if expect == 0 {
            assert_ready_none!(map.poll_next());
        } else {
            assert_pending!(map.poll_next());

            assert_eq!(expect, map.values().count());

            for stream in map.values() {
                assert!(stream.did_poll);
            }
        }
    }
});
