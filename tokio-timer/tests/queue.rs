extern crate futures;
extern crate tokio_executor;
extern crate tokio_timer;
extern crate tokio_mock_task;

#[macro_use]
mod support;
use support::*;

use tokio_timer::*;
use tokio_mock_task::MockTask;

use futures::Stream;

#[test]
fn single_immediate_delay() {
    mocked(|_timer, time| {
        let mut queue = DelayQueue::new();
        let _key = queue.insert_at("foo", time.now());

        let entry = assert_ready!(queue).unwrap();
        assert_eq!(*entry.get_ref(), "foo");

        let entry = assert_ready!(queue);
        assert!(entry.is_none())
    });
}

#[test]
fn multi_immediate_delays() {
    mocked(|_timer, time| {
        let mut queue = DelayQueue::new();

        let _k = queue.insert_at("1", time.now());
        let _k = queue.insert_at("2", time.now());
        let _k = queue.insert_at("3", time.now());

        let mut res = vec![];

        while res.len() < 3 {
            let entry = assert_ready!(queue).unwrap();
            res.push(entry.into_inner());
        }

        let entry = assert_ready!(queue);
        assert!(entry.is_none());

        res.sort();

        assert_eq!("1", res[0]);
        assert_eq!("2", res[1]);
        assert_eq!("3", res[2]);
    });
}

#[test]
fn single_short_delay() {
    mocked(|timer, time| {
        let mut queue = DelayQueue::new();
        let _key = queue.insert_at("foo", time.now() + ms(5));

        let mut task = MockTask::new();

        task.enter(|| {
            assert_not_ready!(queue);
        });

        turn(timer, ms(1));

        assert!(!task.is_notified());

        turn(timer, ms(5));

        assert!(task.is_notified());

        let entry = assert_ready!(queue).unwrap();
        assert_eq!(*entry.get_ref(), "foo");

        let entry = assert_ready!(queue);
        assert!(entry.is_none());
    });
}

#[test]
fn multi_delay_at_start() {
    let long = 262_144 + 9 * 4096;
    let delays = &[1000, 2, 234, long, 60, 10];

    mocked(|timer, time| {
        let mut queue = DelayQueue::new();
        let mut task = MockTask::new();

        // Setup the delays
        for &i in delays {
            let _key = queue.insert_at(i, time.now() + ms(i));
        }

        task.enter(|| {
            assert_not_ready!(queue);
        });

        assert!(!task.is_notified());

        for elapsed in 0..1200 {
            turn(timer, ms(1));
            let elapsed = elapsed + 1;

            if delays.contains(&elapsed) {
                assert!(task.is_notified());

                task.enter(|| {
                    assert_ready!(queue);
                    assert_not_ready!(queue);
                });
            } else {
                if task.is_notified() {
                    let cascade = &[192, 960];
                    assert!(cascade.contains(&elapsed), "elapsed={}", elapsed);

                    task.enter(|| {
                        assert_not_ready!(queue, "elapsed={}", elapsed);
                    });
                }
            }
        }
    });
}

#[test]
fn insert_in_past_fires_immediately() {
    mocked(|timer, time| {
        let mut queue = DelayQueue::new();

        let now = time.now();

        turn(timer, ms(10));

        queue.insert_at("foo", now);

        assert_ready!(queue);
    });
}

#[test]
fn remove_entry() {
    mocked(|timer, time| {
        let mut queue = DelayQueue::new();
        let mut task = MockTask::new();

        let key = queue.insert_at("foo", time.now() + ms(5));

        task.enter(|| {
            assert_not_ready!(queue);
        });

        let entry = queue.remove(&key);
        assert_eq!(entry.into_inner(), "foo");

        turn(timer, ms(10));

        task.enter(|| {
            let entry = assert_ready!(queue);
            assert!(entry.is_none());
        });
    });
}

#[test]
fn reset_entry() {
    mocked(|timer, time| {
        let mut queue = DelayQueue::new();
        let mut task = MockTask::new();

        let now = time.now();
        let key = queue.insert_at("foo", now + ms(5));

        task.enter(|| {
            assert_not_ready!(queue);
        });

        turn(timer, ms(1));

        queue.reset_at(&key, now + ms(10));

        task.enter(|| {
            assert_not_ready!(queue);
        });

        turn(timer, ms(7));

        assert!(!task.is_notified());

        task.enter(|| {
            assert_not_ready!(queue);
        });

        turn(timer, ms(3));

        assert!(task.is_notified());

        let entry = assert_ready!(queue).unwrap();
        assert_eq!(*entry.get_ref(), "foo");

        let entry = assert_ready!(queue);
        assert!(entry.is_none())
    });
}

#[test]
fn reset_much_later() {
    // Reproduces tokio-rs/tokio#849.
    mocked(|timer, time| {
        let mut queue = DelayQueue::new();
        let mut task = MockTask::new();

        let epoch = time.now();

        turn(timer, ms(1));

        let key = queue.insert_at("foo", epoch + ms(200));

        task.enter(|| {
            assert_not_ready!(queue);
        });

        turn(timer, ms(3));

        queue.reset_at(&key, epoch + ms(5));

        turn(timer, ms(20));

        assert!(task.is_notified());
    });
}

#[test]
fn reset_twice() {
    // Reproduces tokio-rs/tokio#849.
    mocked(|timer, time| {
        let mut queue = DelayQueue::new();
        let mut task = MockTask::new();

        let epoch = time.now();

        turn(timer, ms(1));

        let key = queue.insert_at("foo", epoch + ms(200));

        task.enter(|| {
            assert_not_ready!(queue);
        });

        turn(timer, ms(3));

        queue.reset_at(&key, epoch + ms(50));

        turn(timer, ms(20));

        queue.reset_at(&key, epoch + ms(40));

        turn(timer, ms(20));

        assert!(task.is_notified());
    });
}

#[test]
fn remove_expired_item() {
    mocked(|timer, time| {
        let mut queue = DelayQueue::new();

        let now = time.now();

        turn(timer, ms(10));

        let key = queue.insert_at("foo", now);

        let entry = queue.remove(&key);
        assert_eq!(entry.into_inner(), "foo");
    })
}
