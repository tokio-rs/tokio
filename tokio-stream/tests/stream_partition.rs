use tokio_stream::{self as stream, StreamExt};
use tokio_test::{assert_pending, assert_ready_eq, task};

mod support {
    pub(crate) mod mpsc;
}

#[tokio::test]
async fn partition() {
    let stream = stream::iter(0..6);
    let (matches, non_matches) = stream.partition(|v| v % 2 == 0);
    let mut matches = task::spawn(matches);
    let mut non_matches = task::spawn(non_matches);

    // polling matches when the next item matches returns the item from the stream.
    assert_ready_eq!(matches.poll_next(), Some(0));

    // polling non_matches when the next item doesn't match returns the item from the stream.
    assert_ready_eq!(non_matches.poll_next(), Some(1));

    // polling non_matches when the next item matches buffers the item.
    assert_pending!(non_matches.poll_next());

    // polling matches when there is a bufferred match returns the buffered item.
    assert_ready_eq!(matches.poll_next(), Some(2));

    // polling matches when the next item doesn't match buffers the item.
    assert_pending!(matches.poll_next());

    // polling non_matches when there is a bufferred non-match returns the buffered item.
    assert_ready_eq!(non_matches.poll_next(), Some(3));

    // polling non_matches twice when the next item matches buffers the item only once.
    assert_pending!(non_matches.poll_next());
    assert_pending!(non_matches.poll_next());
    assert_ready_eq!(matches.poll_next(), Some(4));

    // polling matches twice when the next item doesn't match buffers the item only once.
    assert_pending!(matches.poll_next());
    assert_pending!(matches.poll_next());
    assert_ready_eq!(non_matches.poll_next(), Some(5));

    // polling matches and non_matches when the stream is exhausted returns None.
    assert_ready_eq!(matches.poll_next(), None);
    assert_ready_eq!(non_matches.poll_next(), None);
}
