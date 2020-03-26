#![warn(rust_2018_idioms)]
#![cfg(feature = "full")]

use tokio::time::{self, delay_for, DelayQueue, Duration, Instant};
use tokio_test::{assert_ok, assert_pending, assert_ready, task};

macro_rules! poll {
    ($queue:ident) => {
        $queue.enter(|cx, mut queue| queue.poll_expired(cx))
    };
}

macro_rules! assert_ready_ok {
    ($e:expr) => {{
        assert_ok!(match assert_ready!($e) {
            Some(v) => v,
            None => panic!("None"),
        })
    }};
}

#[tokio::test]
async fn single_immediate_delay() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());
    let _key = queue.insert_at("foo", Instant::now());

    // Advance time by 1ms to handle thee rounding
    delay_for(ms(1)).await;

    assert_ready_ok!(poll!(queue));

    let entry = assert_ready!(poll!(queue));
    assert!(entry.is_none())
}

#[tokio::test]
async fn multi_immediate_delays() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let _k = queue.insert_at("1", Instant::now());
    let _k = queue.insert_at("2", Instant::now());
    let _k = queue.insert_at("3", Instant::now());

    delay_for(ms(1)).await;

    let mut res = vec![];

    while res.len() < 3 {
        let entry = assert_ready_ok!(poll!(queue));
        res.push(entry.into_inner());
    }

    let entry = assert_ready!(poll!(queue));
    assert!(entry.is_none());

    res.sort();

    assert_eq!("1", res[0]);
    assert_eq!("2", res[1]);
    assert_eq!("3", res[2]);
}

#[tokio::test]
async fn single_short_delay() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());
    let _key = queue.insert_at("foo", Instant::now() + ms(5));

    assert_pending!(poll!(queue));

    delay_for(ms(1)).await;

    assert!(!queue.is_woken());

    delay_for(ms(5)).await;

    assert!(queue.is_woken());

    let entry = assert_ready_ok!(poll!(queue));
    assert_eq!(*entry.get_ref(), "foo");

    let entry = assert_ready!(poll!(queue));
    assert!(entry.is_none());
}

#[tokio::test]
async fn multi_delay_at_start() {
    time::pause();

    let long = 262_144 + 9 * 4096;
    let delays = &[1000, 2, 234, long, 60, 10];

    let mut queue = task::spawn(DelayQueue::new());

    // Setup the delays
    for &i in delays {
        let _key = queue.insert_at(i, Instant::now() + ms(i));
    }

    assert_pending!(poll!(queue));
    assert!(!queue.is_woken());

    for elapsed in 0..1200 {
        delay_for(ms(1)).await;
        let elapsed = elapsed + 1;

        if delays.contains(&elapsed) {
            assert!(queue.is_woken());
            assert_ready!(poll!(queue));
            assert_pending!(poll!(queue));
        } else if queue.is_woken() {
            let cascade = &[192, 960];
            assert!(cascade.contains(&elapsed), "elapsed={}", elapsed);

            assert_pending!(poll!(queue));
        }
    }
}

#[tokio::test]
async fn insert_in_past_fires_immediately() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());
    let now = Instant::now();

    delay_for(ms(10)).await;

    queue.insert_at("foo", now);

    assert_ready!(poll!(queue));
}

#[tokio::test]
async fn remove_entry() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let key = queue.insert_at("foo", Instant::now() + ms(5));

    assert_pending!(poll!(queue));

    let entry = queue.remove(&key);
    assert_eq!(entry.into_inner(), "foo");

    delay_for(ms(10)).await;

    let entry = assert_ready!(poll!(queue));
    assert!(entry.is_none());
}

#[tokio::test]
async fn reset_entry() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();
    let key = queue.insert_at("foo", now + ms(5));

    assert_pending!(poll!(queue));
    delay_for(ms(1)).await;

    queue.reset_at(&key, now + ms(10));

    assert_pending!(poll!(queue));

    delay_for(ms(7)).await;

    assert!(!queue.is_woken());

    assert_pending!(poll!(queue));

    delay_for(ms(3)).await;

    assert!(queue.is_woken());

    let entry = assert_ready_ok!(poll!(queue));
    assert_eq!(*entry.get_ref(), "foo");

    let entry = assert_ready!(poll!(queue));
    assert!(entry.is_none())
}

// Reproduces tokio-rs/tokio#849.
#[tokio::test]
async fn reset_much_later() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();
    delay_for(ms(1)).await;

    let key = queue.insert_at("foo", now + ms(200));
    assert_pending!(poll!(queue));

    delay_for(ms(3)).await;

    queue.reset_at(&key, now + ms(5));

    delay_for(ms(20)).await;

    assert!(queue.is_woken());
}

// Reproduces tokio-rs/tokio#849.
#[tokio::test]
async fn reset_twice() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());
    let now = Instant::now();

    delay_for(ms(1)).await;

    let key = queue.insert_at("foo", now + ms(200));

    assert_pending!(poll!(queue));

    delay_for(ms(3)).await;

    queue.reset_at(&key, now + ms(50));

    delay_for(ms(20)).await;

    queue.reset_at(&key, now + ms(40));

    delay_for(ms(20)).await;

    assert!(queue.is_woken());
}

#[tokio::test]
async fn remove_expired_item() {
    time::pause();

    let mut queue = DelayQueue::new();

    let now = Instant::now();

    delay_for(ms(10)).await;

    let key = queue.insert_at("foo", now);

    let entry = queue.remove(&key);
    assert_eq!(entry.into_inner(), "foo");
}

#[tokio::test]
async fn expires_before_last_insert() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();

    queue.insert_at("foo", now + ms(10_000));

    // Delay should be set to 8.192s here.
    assert_pending!(poll!(queue));

    // Delay should be set to the delay of the new item here
    queue.insert_at("bar", now + ms(600));

    assert_pending!(poll!(queue));

    delay_for(ms(600)).await;

    assert!(queue.is_woken());

    let entry = assert_ready_ok!(poll!(queue)).into_inner();
    assert_eq!(entry, "bar");
}

#[tokio::test]
async fn multi_reset() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();

    let one = queue.insert_at("one", now + ms(200));
    let two = queue.insert_at("two", now + ms(250));

    assert_pending!(poll!(queue));

    queue.reset_at(&one, now + ms(300));
    queue.reset_at(&two, now + ms(350));
    queue.reset_at(&one, now + ms(400));

    delay_for(ms(310)).await;

    assert_pending!(poll!(queue));

    delay_for(ms(50)).await;

    let entry = assert_ready_ok!(poll!(queue));
    assert_eq!(*entry.get_ref(), "two");

    assert_pending!(poll!(queue));

    delay_for(ms(50)).await;

    let entry = assert_ready_ok!(poll!(queue));
    assert_eq!(*entry.get_ref(), "one");

    let entry = assert_ready!(poll!(queue));
    assert!(entry.is_none())
}

#[tokio::test]
async fn expire_first_key_when_reset_to_expire_earlier() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();

    let one = queue.insert_at("one", now + ms(200));
    queue.insert_at("two", now + ms(250));

    assert_pending!(poll!(queue));

    queue.reset_at(&one, now + ms(100));

    delay_for(ms(100)).await;

    assert!(queue.is_woken());

    let entry = assert_ready_ok!(poll!(queue)).into_inner();
    assert_eq!(entry, "one");
}

#[tokio::test]
async fn expire_second_key_when_reset_to_expire_earlier() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();

    queue.insert_at("one", now + ms(200));
    let two = queue.insert_at("two", now + ms(250));

    assert_pending!(poll!(queue));

    queue.reset_at(&two, now + ms(100));

    delay_for(ms(100)).await;

    assert!(queue.is_woken());

    let entry = assert_ready_ok!(poll!(queue)).into_inner();
    assert_eq!(entry, "two");
}

#[tokio::test]
async fn reset_first_expiring_item_to_expire_later() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();

    let one = queue.insert_at("one", now + ms(200));
    let _two = queue.insert_at("two", now + ms(250));

    assert_pending!(poll!(queue));

    queue.reset_at(&one, now + ms(300));
    delay_for(ms(250)).await;

    assert!(queue.is_woken());

    let entry = assert_ready_ok!(poll!(queue)).into_inner();
    assert_eq!(entry, "two");
}

#[tokio::test]
async fn insert_before_first_after_poll() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();

    let _one = queue.insert_at("one", now + ms(200));

    assert_pending!(poll!(queue));

    let _two = queue.insert_at("two", now + ms(100));

    delay_for(ms(99)).await;

    assert!(!queue.is_woken());

    delay_for(ms(1)).await;

    assert!(queue.is_woken());

    let entry = assert_ready_ok!(poll!(queue)).into_inner();
    assert_eq!(entry, "two");
}

#[tokio::test]
async fn insert_after_ready_poll() {
    time::pause();

    let mut queue = task::spawn(DelayQueue::new());

    let now = Instant::now();

    queue.insert_at("1", now + ms(100));
    queue.insert_at("2", now + ms(100));
    queue.insert_at("3", now + ms(100));

    assert_pending!(poll!(queue));

    delay_for(ms(100)).await;

    assert!(queue.is_woken());

    let mut res = vec![];

    while res.len() < 3 {
        let entry = assert_ready_ok!(poll!(queue));
        res.push(entry.into_inner());
        queue.insert_at("foo", now + ms(500));
    }

    res.sort();

    assert_eq!("1", res[0]);
    assert_eq!("2", res[1]);
    assert_eq!("3", res[2]);
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
