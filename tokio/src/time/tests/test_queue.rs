#![warn(rust_2018_idioms)]

use crate::time::tests::mock_clock::mock;
use crate::time::{DelayQueue, Duration};
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

#[test]
fn single_immediate_delay() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());
        let _key = queue.insert_at("foo", clock.now());

        let entry = assert_ready_ok!(poll!(queue));
        assert_eq!(*entry.get_ref(), "foo");

        let entry = assert_ready!(poll!(queue));
        assert!(entry.is_none())
    });
}

#[test]
fn multi_immediate_delays() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let _k = queue.insert_at("1", clock.now());
        let _k = queue.insert_at("2", clock.now());
        let _k = queue.insert_at("3", clock.now());

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
    });
}

#[test]
fn single_short_delay() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());
        let _key = queue.insert_at("foo", clock.now() + ms(5));

        assert_pending!(poll!(queue));

        clock.turn_for(ms(1));

        assert!(!queue.is_woken());

        clock.turn_for(ms(5));

        assert!(queue.is_woken());

        let entry = assert_ready_ok!(poll!(queue));
        assert_eq!(*entry.get_ref(), "foo");

        let entry = assert_ready!(poll!(queue));
        assert!(entry.is_none());
    });
}

#[test]
fn multi_delay_at_start() {
    let long = 262_144 + 9 * 4096;
    let delays = &[1000, 2, 234, long, 60, 10];

    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        // Setup the delays
        for &i in delays {
            let _key = queue.insert_at(i, clock.now() + ms(i));
        }

        assert_pending!(poll!(queue));
        assert!(!queue.is_woken());

        for elapsed in 0..1200 {
            clock.turn_for(ms(1));
            let elapsed = elapsed + 1;

            if delays.contains(&elapsed) {
                assert!(queue.is_woken());
                assert_ready!(poll!(queue));
                assert_pending!(poll!(queue));
            } else {
                if queue.is_woken() {
                    let cascade = &[192, 960];
                    assert!(cascade.contains(&elapsed), "elapsed={}", elapsed);

                    assert_pending!(poll!(queue));
                }
            }
        }
    });
}

#[test]
fn insert_in_past_fires_immediately() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let now = clock.now();

        clock.turn_for(ms(10));

        queue.insert_at("foo", now);

        assert_ready!(poll!(queue));
    });
}

#[test]
fn remove_entry() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let key = queue.insert_at("foo", clock.now() + ms(5));

        assert_pending!(poll!(queue));

        let entry = queue.remove(&key);
        assert_eq!(entry.into_inner(), "foo");

        clock.turn_for(ms(10));

        let entry = assert_ready!(poll!(queue));
        assert!(entry.is_none());
    });
}

#[test]
fn reset_entry() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let now = clock.now();
        let key = queue.insert_at("foo", now + ms(5));

        assert_pending!(poll!(queue));
        clock.turn_for(ms(1));

        queue.reset_at(&key, now + ms(10));

        assert_pending!(poll!(queue));

        clock.turn_for(ms(7));

        assert!(!queue.is_woken());

        assert_pending!(poll!(queue));

        clock.turn_for(ms(3));

        assert!(queue.is_woken());

        let entry = assert_ready_ok!(poll!(queue));
        assert_eq!(*entry.get_ref(), "foo");

        let entry = assert_ready!(poll!(queue));
        assert!(entry.is_none())
    });
}

#[test]
fn reset_much_later() {
    // Reproduces tokio-rs/tokio#849.
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let epoch = clock.now();

        clock.turn_for(ms(1));

        let key = queue.insert_at("foo", epoch + ms(200));

        assert_pending!(poll!(queue));

        clock.turn_for(ms(3));

        queue.reset_at(&key, epoch + ms(5));

        clock.turn_for(ms(20));

        assert!(queue.is_woken());
    });
}

#[test]
fn reset_twice() {
    // Reproduces tokio-rs/tokio#849.
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let epoch = clock.now();

        clock.turn_for(ms(1));

        let key = queue.insert_at("foo", epoch + ms(200));

        assert_pending!(poll!(queue));

        clock.turn_for(ms(3));

        queue.reset_at(&key, epoch + ms(50));

        clock.turn_for(ms(20));

        queue.reset_at(&key, epoch + ms(40));

        clock.turn_for(ms(20));

        assert!(queue.is_woken());
    });
}

#[test]
fn remove_expired_item() {
    mock(|clock| {
        let mut queue = DelayQueue::new();

        let now = clock.now();

        clock.turn_for(ms(10));

        let key = queue.insert_at("foo", now);

        let entry = queue.remove(&key);
        assert_eq!(entry.into_inner(), "foo");
    })
}

#[test]
fn expires_before_last_insert() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let epoch = clock.now();

        queue.insert_at("foo", epoch + ms(10_000));

        // Delay should be set to 8.192s here.
        assert_pending!(poll!(queue));

        // Delay should be set to the delay of the new item here
        queue.insert_at("bar", epoch + ms(600));

        assert_pending!(poll!(queue));

        clock.advance(ms(600));

        assert!(queue.is_woken());

        let entry = assert_ready_ok!(poll!(queue)).into_inner();
        assert_eq!(entry, "bar");
    })
}

#[test]
fn multi_reset() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let epoch = clock.now();

        let foo = queue.insert_at("foo", epoch + ms(200));
        let bar = queue.insert_at("bar", epoch + ms(250));

        assert_pending!(poll!(queue));

        queue.reset_at(&foo, epoch + ms(300));
        queue.reset_at(&bar, epoch + ms(350));
        queue.reset_at(&foo, epoch + ms(400));
    })
}

#[test]
fn expire_first_key_when_reset_to_expire_earlier() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let epoch = clock.now();

        let foo = queue.insert_at("foo", epoch + ms(200));
        queue.insert_at("bar", epoch + ms(250));

        assert_pending!(poll!(queue));

        queue.reset_at(&foo, epoch + ms(100));

        clock.advance(ms(100));

        assert!(queue.is_woken());

        let entry = assert_ready_ok!(poll!(queue)).into_inner();
        assert_eq!(entry, "foo");
    })
}

#[test]
fn expire_second_key_when_reset_to_expire_earlier() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let epoch = clock.now();

        queue.insert_at("foo", epoch + ms(200));
        let bar = queue.insert_at("bar", epoch + ms(250));

        assert_pending!(poll!(queue));

        queue.reset_at(&bar, epoch + ms(100));

        clock.advance(ms(100));

        assert!(queue.is_woken());
        let entry = assert_ready_ok!(poll!(queue)).into_inner();
        assert_eq!(entry, "bar");
    })
}

#[test]
fn reset_first_expiring_item_to_expire_later() {
    mock(|clock| {
        let mut queue = task::spawn(DelayQueue::new());

        let epoch = clock.now();

        let foo = queue.insert_at("foo", epoch + ms(200));
        let _bar = queue.insert_at("bar", epoch + ms(250));

        assert_pending!(poll!(queue));

        queue.reset_at(&foo, epoch + ms(300));
        clock.advance(ms(250));

        assert!(queue.is_woken());

        let entry = assert_ready_ok!(poll!(queue)).into_inner();
        assert_eq!(entry, "bar");
    })
}

fn ms(n: u64) -> Duration {
    Duration::from_millis(n)
}
