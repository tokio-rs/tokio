use std::pin::Pin;

use tokio_stream::{self as stream, Iter, StreamExt};

fn base() -> Iter<std::vec::IntoIter<i32>> {
    stream::iter(vec![1, 2, 3])
}

#[tokio::test]
async fn map_accessors() {
    let mut map = base().map(|x| x * 2);

    let _: &Iter<_> = map.get_ref();
    let _: &mut Iter<_> = map.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut map).get_pin_mut();

    assert_eq!(map.next().await, Some(2));

    // The recovered inner stream continues from where the combinator left it.
    let mut inner = map.into_inner();
    assert_eq!(inner.next().await, Some(2));
    assert_eq!(inner.next().await, Some(3));
    assert_eq!(inner.next().await, None);
}

#[tokio::test]
async fn take_accessors() {
    let mut take = base().take(2);

    let _: &Iter<_> = take.get_ref();
    let _: &mut Iter<_> = take.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut take).get_pin_mut();

    assert_eq!(take.next().await, Some(1));

    // `take(2)` would stop after item 2, but the inner stream still has
    // everything that was not yet pulled.
    let mut inner = take.into_inner();
    assert_eq!(inner.next().await, Some(2));
    assert_eq!(inner.next().await, Some(3));
}

#[tokio::test]
async fn skip_accessors() {
    let mut skip = base().skip(1);

    let _: &Iter<_> = skip.get_ref();
    let _: &mut Iter<_> = skip.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut skip).get_pin_mut();

    assert_eq!(skip.next().await, Some(2));

    let mut inner = skip.into_inner();
    assert_eq!(inner.next().await, Some(3));
}

#[tokio::test]
async fn filter_accessors() {
    let mut filter = base().filter(|&x| x % 2 == 1);

    let _: &Iter<_> = filter.get_ref();
    let _: &mut Iter<_> = filter.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut filter).get_pin_mut();

    assert_eq!(filter.next().await, Some(1));

    let mut inner = filter.into_inner();
    assert_eq!(inner.next().await, Some(2));
}

#[tokio::test]
async fn filter_map_accessors() {
    let mut filter_map = base().filter_map(|x| (x % 2 == 1).then_some(x * 10));

    let _: &Iter<_> = filter_map.get_ref();
    let _: &mut Iter<_> = filter_map.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut filter_map).get_pin_mut();

    assert_eq!(filter_map.next().await, Some(10));

    let mut inner = filter_map.into_inner();
    assert_eq!(inner.next().await, Some(2));
}

#[tokio::test]
async fn map_while_accessors() {
    let mut map_while = base().map_while(|x| (x < 3).then_some(x + 100));

    let _: &Iter<_> = map_while.get_ref();
    let _: &mut Iter<_> = map_while.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut map_while).get_pin_mut();

    assert_eq!(map_while.next().await, Some(101));

    let mut inner = map_while.into_inner();
    assert_eq!(inner.next().await, Some(2));
}

#[tokio::test]
async fn take_while_accessors() {
    let mut take_while = base().take_while(|&x| x < 3);

    let _: &Iter<_> = take_while.get_ref();
    let _: &mut Iter<_> = take_while.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut take_while).get_pin_mut();

    assert_eq!(take_while.next().await, Some(1));

    let mut inner = take_while.into_inner();
    assert_eq!(inner.next().await, Some(2));
}

#[tokio::test]
async fn skip_while_accessors() {
    let mut skip_while = base().skip_while(|&x| x < 2);

    let _: &Iter<_> = skip_while.get_ref();
    let _: &mut Iter<_> = skip_while.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut skip_while).get_pin_mut();

    assert_eq!(skip_while.next().await, Some(2));

    let mut inner = skip_while.into_inner();
    assert_eq!(inner.next().await, Some(3));
}

#[tokio::test]
async fn then_accessors() {
    let mut then = base().then(|x| std::future::ready(x + 1));

    let _: &Iter<_> = then.get_ref();
    let _: &mut Iter<_> = then.get_mut();
    let _: Pin<&mut Iter<_>> = Pin::new(&mut then).get_pin_mut();

    assert_eq!(then.next().await, Some(2));

    let mut inner = then.into_inner();
    assert_eq!(inner.next().await, Some(2));
}

#[tokio::test]
async fn get_mut_can_mutate_inner() {
    // Mutating through `get_mut` is observed by the combinator.
    let mut map = base().map(|x| x * 2);

    // Skip one item directly on the inner stream.
    let inner = map.get_mut();
    assert_eq!(inner.next().await, Some(1));

    // The combinator now sees the stream from item 2 onwards.
    assert_eq!(map.next().await, Some(4));
}
