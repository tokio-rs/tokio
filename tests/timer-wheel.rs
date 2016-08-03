extern crate env_logger;
extern crate futures_mio;

use std::time::{Instant, Duration};

use futures_mio::timer_wheel::TimerWheel;

fn ms(amt: u64) -> Duration {
    Duration::from_millis(amt)
}

#[test]
fn smoke() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    assert!(timer.poll(now).is_none());
    assert!(timer.poll(now).is_none());

    timer.insert(now + ms(200), 3);

    assert!(timer.poll(now).is_none());
    assert!(timer.poll(now + ms(100)).is_none());
    let res = timer.poll(now + ms(200));
    assert!(res.is_some());
    assert_eq!(res.unwrap(), 3);
}

#[test]
fn poll_past_done() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    timer.insert(now + ms(200), 3);
    let res = timer.poll(now + ms(300));
    assert!(res.is_some());
    assert_eq!(res.unwrap(), 3);
}

#[test]
fn multiple_ready() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    timer.insert(now + ms(200), 3);
    timer.insert(now + ms(201), 4);
    timer.insert(now + ms(202), 5);
    timer.insert(now + ms(300), 6);
    timer.insert(now + ms(301), 7);

    let mut found = Vec::new();
    while let Some(i) = timer.poll(now + ms(400)) {
        found.push(i);
    }
    found.sort();
    assert_eq!(found, [3, 4, 5, 6, 7]);
}

#[test]
fn poll_now() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    timer.insert(now, 3);
    let res = timer.poll(now + ms(100));
    assert!(res.is_some());
    assert_eq!(res.unwrap(), 3);
}

#[test]
fn cancel() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    let timeout = timer.insert(now + ms(800), 3);
    assert!(timer.poll(now + ms(200)).is_none());
    assert!(timer.poll(now + ms(400)).is_none());
    assert_eq!(timer.cancel(&timeout), Some(3));
    assert!(timer.poll(now + ms(600)).is_none());
    assert!(timer.poll(now + ms(800)).is_none());
    assert!(timer.poll(now + ms(1000)).is_none());
}

#[test]
fn next_timeout() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    assert!(timer.next_timeout().is_none());
    timer.insert(now + ms(400), 3);
    let timeout = timer.next_timeout().expect("wanted a next_timeout");
    assert_eq!(timeout, now + ms(400));

    timer.insert(now + ms(1000), 3);
    let timeout = timer.next_timeout().expect("wanted a next_timeout");
    assert_eq!(timeout, now + ms(400));
}

#[test]
fn around_the_boundary() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    timer.insert(now + ms(199), 3);
    timer.insert(now + ms(200), 4);
    timer.insert(now + ms(201), 5);
    timer.insert(now + ms(251), 6);

    let mut found = Vec::new();
    while let Some(i) = timer.poll(now + ms(200)) {
        found.push(i);
    }
    found.sort();
    assert_eq!(found, [3, 4, 5]);

    assert_eq!(timer.poll(now + ms(300)), Some(6));
    assert_eq!(timer.poll(now + ms(300)), None);
}

#[test]
fn remove_clears_timeout() {
    drop(env_logger::init());
    let mut timer = TimerWheel::<i32>::new();
    let now = Instant::now();

    timer.insert(now + ms(100), 3);
    assert_eq!(timer.next_timeout(), Some(now + ms(100)));
    assert_eq!(timer.poll(now + ms(200)), Some(3));
    assert_eq!(timer.next_timeout(), None);
}
