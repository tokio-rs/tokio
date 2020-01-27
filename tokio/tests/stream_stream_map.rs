use tokio::stream::{self, pending, Stream, StreamExt, StreamMap};
use tokio::sync::mpsc;
use tokio_test::{task, assert_ok, assert_pending, assert_ready};

macro_rules! assert_ready_some {
    ($($t:tt)*) => {
        match assert_ready!($($t)*) {
            Some(v) => v,
            None => panic!("expected `Some`, got `None`"),
        }
    };
}

macro_rules! assert_ready_none {
    ($($t:tt)*) => {
        match assert_ready!($($t)*) {
            None => {}
            Some(v) => panic!("expected `None`, got `Some({:?})`", v),
        }
    };
}

#[tokio::test]
async fn empty() {
    let mut map = StreamMap::<&str, stream::Pending<()>>::new();

    assert_eq!(map.len(), 0);
    assert!(map.is_empty());

    assert!(map.next().await.is_none());
    assert!(map.next().await.is_none());

    assert!(map.remove("foo").is_none());
}

#[tokio::test]
async fn single_entry() {
    let mut map = task::spawn(StreamMap::new());
    let (tx, rx) = mpsc::unbounded_channel();

    assert_ready_none!(map.poll_next());

    assert!(map.insert("foo", rx).is_none());
    assert!(map.contains_key("foo"));
    assert!(!map.contains_key("bar"));

    assert_eq!(map.len(), 1);
    assert!(!map.is_empty());

    assert_pending!(map.poll_next());

    assert_ok!(tx.send(1));

    assert!(map.is_woken());
    let (k, v) = assert_ready_some!(map.poll_next());
    assert_eq!(k, "foo");
    assert_eq!(v, 1);

    assert_pending!(map.poll_next());

    assert_ok!(tx.send(2));

    assert!(map.is_woken());
    let (k, v) = assert_ready_some!(map.poll_next());
    assert_eq!(k, "foo");
    assert_eq!(v, 2);

    assert_pending!(map.poll_next());
    drop(tx);
    assert!(map.is_woken());
    assert_ready_none!(map.poll_next());
}

#[tokio::test]
async fn multiple_entries() {
    let mut map = task::spawn(StreamMap::new());
    let (tx1, rx1) = mpsc::unbounded_channel();
    let (tx2, rx2) = mpsc::unbounded_channel();

    map.insert("foo", rx1);
    map.insert("bar", rx2);

    assert_pending!(map.poll_next());

    assert_ok!(tx1.send(1));

    assert!(map.is_woken());
    let (k, v) = assert_ready_some!(map.poll_next());
    assert_eq!(k, "foo");
    assert_eq!(v, 1);

    assert_pending!(map.poll_next());

    assert_ok!(tx2.send(2));

    assert!(map.is_woken());
    let (k, v) = assert_ready_some!(map.poll_next());
    assert_eq!(k, "bar");
    assert_eq!(v, 2);

    assert_pending!(map.poll_next());

    assert_ok!(tx1.send(3));
    assert_ok!(tx2.send(4));

    assert!(map.is_woken());

    // Given the randomization, there is no guarantee what order the values will
    // be received in.
    let mut v = (0..2)
        .map(|_| assert_ready_some!(map.poll_next()))
        .collect::<Vec<_>>();

    assert_pending!(map.poll_next());

    v.sort();
    assert_eq!(v[0].0, "bar");
    assert_eq!(v[0].1, 4);
    assert_eq!(v[1].0, "foo");
    assert_eq!(v[1].1, 3);

    drop(tx1);
    assert!(map.is_woken());
    assert_pending!(map.poll_next());
    drop(tx2);

    assert_ready_none!(map.poll_next());
}

#[tokio::test]
async fn insert_remove() {
    let mut map = task::spawn(StreamMap::new());
    let (tx, rx) = mpsc::unbounded_channel();

    assert_ready_none!(map.poll_next());

    assert!(map.insert("foo", rx).is_none());
    let rx = map.remove("foo").unwrap();

    assert_ok!(tx.send(1));

    assert!(!map.is_woken());
    assert_ready_none!(map.poll_next());

    assert!(map.insert("bar", rx).is_none());

    let v = assert_ready_some!(map.poll_next());
    assert_eq!(v.0, "bar");
    assert_eq!(v.1, 1);

    assert!(map.remove("bar").is_some());
    assert_ready_none!(map.poll_next());

    assert!(map.is_empty());
    assert_eq!(0, map.len());
}

#[tokio::test]
async fn replace() {
    let mut map = task::spawn(StreamMap::new());
    let (tx1, rx1) = mpsc::unbounded_channel();
    let (tx2, rx2) = mpsc::unbounded_channel();

    assert!(map.insert("foo", rx1).is_none());

    assert_pending!(map.poll_next());

    let _rx1 = map.insert("foo", rx2).unwrap();

    assert_pending!(map.poll_next());

    tx1.send(1).unwrap();
    assert_pending!(map.poll_next());

    tx2.send(2).unwrap();
    assert!(map.is_woken());
    let v = assert_ready_some!(map.poll_next());
    assert_eq!(v.0, "foo");
    assert_eq!(v.1, 2);
}

#[test]
fn size_hint_with_upper() {
    let mut map = StreamMap::new();

    map.insert("a", stream::iter(vec![1]));
    map.insert("b", stream::iter(vec![1, 2]));
    map.insert("c", stream::iter(vec![1, 2, 3]));

    assert_eq!(3, map.len());
    assert!(!map.is_empty());

    let size_hint = map.size_hint();
    assert_eq!(size_hint, (6, Some(6)));
}

#[test]
fn size_hint_without_upper() {
    use std::pin::Pin;

    let mut map = StreamMap::new();

    type B = Pin<Box<dyn Stream<Item = i32>>>;

    map.insert("a", Box::pin(stream::iter(vec![1])) as B);
    map.insert("b", Box::pin(stream::iter(vec![1, 2])) as B);
    map.insert("c", Box::pin(pending()) as B);

    let size_hint = map.size_hint();
    assert_eq!(size_hint, (3, None));
}


#[test]
fn new_capacity_zero() {
    let map = StreamMap::<&str, stream::Pending<()>>::new();
    assert_eq!(0, map.capacity());

    let keys = map.keys().collect::<Vec<_>>();
    assert!(keys.is_empty());
}

#[test]
fn with_capacity() {
    let map = StreamMap::<&str, stream::Pending<()>>::with_capacity(10);
    assert!(10 <= map.capacity());

    let keys = map.keys().collect::<Vec<_>>();
    assert!(keys.is_empty());
}

#[test]
fn iter_keys() {
    let mut map = StreamMap::new();

    map.insert("a", pending::<i32>());
    map.insert("b", pending());
    map.insert("c", pending());

    let mut keys = map.keys().collect::<Vec<_>>();
    keys.sort();

    assert_eq!(&keys[..], &[&"a", &"b", &"c"]);
}

#[test]
fn iter_values() {
    let mut map = StreamMap::new();

    map.insert("a", stream::iter(vec![1]));
    map.insert("b", stream::iter(vec![1, 2]));
    map.insert("c", stream::iter(vec![1, 2, 3]));

    let mut size_hints = map
        .values()
        .map(|s| s.size_hint().0)
        .collect::<Vec<_>>();

    size_hints.sort();

    assert_eq!(&size_hints[..], &[1, 2, 3]);
}

#[test]
fn iter_values_mut() {
    let mut map = StreamMap::new();

    map.insert("a", stream::iter(vec![1]));
    map.insert("b", stream::iter(vec![1, 2]));
    map.insert("c", stream::iter(vec![1, 2, 3]));

    let mut size_hints = map
        .values_mut()
        .map(|s: &mut _| s.size_hint().0)
        .collect::<Vec<_>>();

    size_hints.sort();

    assert_eq!(&size_hints[..], &[1, 2, 3]);
}

#[test]
fn clear() {
    let mut map = task::spawn(StreamMap::new());

    map.insert("a", stream::iter(vec![1]));
    map.insert("b", stream::iter(vec![1, 2]));
    map.insert("c", stream::iter(vec![1, 2, 3]));

    assert_ready_some!(map.poll_next());

    map.clear();

    assert_ready_none!(map.poll_next());
    assert!(map.is_empty());
}

#[test]
fn contains_key_borrow() {
    let mut map = StreamMap::new();
    map.insert("foo".to_string(), pending::<()>());

    assert!(map.contains_key("foo"));
}
