//! A timer wheel implementation

use std::cmp;
use std::mem;
use std::time::{Instant, Duration};

use slab::Slab;

/// An implementation of a timer wheel where data can be associated with each
/// timer firing.
///
/// This structure implements a timer wheel data structure where each timeout
/// has a piece of associated data, `T`. A timer wheel supports O(1) insertion
/// and removal of timers, as well as quickly figuring out what needs to get
/// fired.
///
/// Note, though, that the resolution of a timer wheel means that timeouts will
/// not arrive promptly when they expire, but rather in certain increments of
/// each time. The time delta between each slot of a time wheel is of a fixed
/// length, meaning that if a timeout is scheduled between two slots it'll end
/// up getting scheduled into the later slot.
pub struct TimerWheel<T> {
    // Actual timer wheel itself.
    //
    // Each slot represents a fixed duration of time, and this wheel also
    // behaves like a ring buffer. All timeouts scheduled will correspond to one
    // slot and therefore each slot has a linked list of timeouts scheduled in
    // it. Right now linked lists are done through indices into the `slab`
    // below.
    //
    // Each slot also contains the next timeout associated with it (the minimum
    // of the entire linked list).
    wheel: Vec<Slot>,

    // A slab containing all the timeout entries themselves. This is the memory
    // backing the "linked lists" in the wheel above. Each entry has a prev/next
    // pointer (indices in this array) along with the data associated with the
    // timeout and the time the timeout will fire.
    slab: Slab<Entry<T>, usize>,

    // The instant that this timer was created, through which all other timeout
    // computations are relative to.
    start: Instant,

    // State used during `poll`. The `cur_wheel_tick` field is the current tick
    // we've poll'd to. That is, all events from `cur_wheel_tick` to the
    // actual current tick in time still need to be processed.
    //
    // The `cur_slab_idx` variable is basically just an iterator over the linked
    // list associated with a wheel slot. This will get incremented as we move
    // forward in `poll`
    cur_wheel_tick: u64,
    cur_slab_idx: usize,
}

#[derive(Clone)]
struct Slot {
    head: usize,
    next_timeout: Option<Instant>,
}

struct Entry<T> {
    data: T,
    when: Instant,
    wheel_idx: usize,
    prev: usize,
    next: usize,
}

/// A timeout which has been scheduled with a timer wheel.
///
/// This can be used to later cancel a timeout, if necessary.
pub struct Timeout {
    when: Instant,
    slab_idx: usize,
}

const EMPTY: usize = 0;
const LEN: usize = 256;
const MASK: usize = LEN - 1;
const TICK_MS: u64 = 100;

impl<T> TimerWheel<T> {
    /// Creates a new timer wheel configured with no timeouts and with the
    /// default parameters.
    ///
    /// Currently this is a timer wheel of length 256 with a 100ms time
    /// resolution.
    pub fn new() -> TimerWheel<T> {
        TimerWheel {
            wheel: vec![Slot { head: EMPTY, next_timeout: None }; LEN],
            slab: Slab::new_starting_at(1, 256),
            start: Instant::now(),
            cur_wheel_tick: 0,
            cur_slab_idx: EMPTY,
        }
    }

    /// Creates a new timeout to get fired at a particular point in the future.
    ///
    /// The timeout will be associated with the specified `data`, and this data
    /// will be returned from `poll` when it's ready.
    ///
    /// The returned `Timeout` can later get passesd to `cancel` to retrieve the
    /// data and ensure the timeout doesn't fire.
    ///
    /// This method completes in O(1) time.
    ///
    /// # Panics
    ///
    /// This method will panic if `at` is before the time that this timer wheel
    /// was created.
    pub fn insert(&mut self, at: Instant, data: T) -> Timeout {
        // First up, figure out where we're gonna go in the wheel. Note that if
        // we're being scheduled on or before the current wheel tick we just
        // make sure to defer ourselves to the next tick.
        let mut tick = self.time_to_ticks(at);
        if tick <= self.cur_wheel_tick {
            debug!("moving {} to {}", tick, self.cur_wheel_tick + 1);
            tick = self.cur_wheel_tick + 1;
        }
        let wheel_idx = self.ticks_to_wheel_idx(tick);
        trace!("inserting timeout at {} for {}", wheel_idx, tick);

        // Next, make sure there's enough space in the slab for the timeout.
        if self.slab.vacant_entry().is_none() {
            let amt = self.slab.count();
            self.slab.grow(amt);
        }

        // Insert ourselves at the head of the linked list in the wheel.
        let slot = &mut self.wheel[wheel_idx];
        let prev_head;
        {
            let entry = self.slab.vacant_entry().unwrap();
            prev_head = mem::replace(&mut slot.head, entry.index());
            trace!("timer wheel slab idx: {}", entry.index());

            entry.insert(Entry {
                data: data,
                when: at,
                wheel_idx: wheel_idx,
                prev: EMPTY,
                next: prev_head,
            });
        }
        if prev_head != EMPTY {
            self.slab[prev_head].prev = slot.head;
        }

        // Update the wheel slot's next timeout field.
        if at <= slot.next_timeout.unwrap_or(at) {
            let tick = tick as u32;
            let actual_tick = self.start + Duration::from_millis(TICK_MS) * tick;
            trace!("actual_tick: {:?}", actual_tick);
            trace!("at:          {:?}", at);
            let at = cmp::max(actual_tick, at);
            debug!("updating[{}] next timeout: {:?}", wheel_idx, at);
            slot.next_timeout = Some(at);
        }

        Timeout {
            when: at,
            slab_idx: slot.head,
        }
    }

    /// Queries this timer to see if any timeouts are ready to fire.
    ///
    /// This function will advance the internal wheel to the time specified by
    /// `at`, returning any timeout which has happened up to that point. This
    /// method should be called in a loop until it returns `None` to ensure that
    /// all timeouts are processed.
    ///
    /// # Panics
    ///
    /// This method will panic if `at` is before the instant that this timer
    /// wheel was created.
    pub fn poll(&mut self, at: Instant) -> Option<T> {
        let wheel_tick = self.time_to_ticks(at);

        trace!("polling {} => {}", self.cur_wheel_tick, wheel_tick);

        // Advance forward in time to the `wheel_tick` specified.
        //
        // TODO: don't visit slots in the wheel more than once
        while self.cur_wheel_tick <= wheel_tick {
            let head = self.cur_slab_idx;
            let idx = self.ticks_to_wheel_idx(self.cur_wheel_tick);
            trace!("next head[{} => {}]: {}",
                   self.cur_wheel_tick, wheel_tick, head);

            // If the current slot has no entries or we're done iterating go to
            // the next tick.
            if head == EMPTY {
                if head == self.wheel[idx].head {
                    self.wheel[idx].next_timeout = None;
                }
                self.cur_wheel_tick += 1;
                let idx = self.ticks_to_wheel_idx(self.cur_wheel_tick);
                self.cur_slab_idx = self.wheel[idx].head;
                continue
            }

            // If we're starting to iterate over a slot, clear its timeout as
            // we're probably going to remove entries. As we skip over each
            // element of this slot we'll restore the `next_timeout` field if
            // necessary.
            if head == self.wheel[idx].head {
                self.wheel[idx].next_timeout = None;
            }

            // Otherwise, continue iterating over the linked list in the wheel
            // slot we're on and remove anything which has expired.
            self.cur_slab_idx = self.slab[head].next;
            let head_timeout = self.slab[head].when;
            if self.time_to_ticks(head_timeout) <= self.time_to_ticks(at) {
                return self.remove_slab(head).map(|e| e.data)
            } else {
                let next = self.wheel[idx].next_timeout.unwrap_or(head_timeout);
                if head_timeout <= next {
                    self.wheel[idx].next_timeout = Some(head_timeout);
                }
            }
        }

        None
    }

    /// Returns the instant in time that corresponds to the next timeout
    /// scheduled in this wheel.
    pub fn next_timeout(&self) -> Option<Instant> {
        // TODO: can this be optimized to not look at the whole array?
        let timeouts = self.wheel.iter().map(|slot| slot.next_timeout);
        let min = timeouts.fold(None, |prev, cur| {
            match (prev, cur) {
                (None, cur) => cur,
                (Some(time), None) => Some(time),
                (Some(a), Some(b)) => Some(cmp::min(a, b)),
            }
        });
        if let Some(min) = min {
            debug!("next timeout {:?}", min);
            debug!("now          {:?}", Instant::now());
        } else {
            debug!("next timeout never");
        }
        return min
    }

    /// Cancels the specified timeout.
    ///
    /// For timeouts previously registered via `insert` they can be passed back
    /// to this method to cancel the associated timeout, retrieving the value
    /// inserted if the timeout has not already fired.
    ///
    /// This method completes in O(1) time.
    ///
    /// # Panics
    ///
    /// This method may panic if `timeout` wasn't created by this timer wheel.
    pub fn cancel(&mut self, timeout: &Timeout) -> Option<T> {
        match self.slab.get(timeout.slab_idx) {
            Some(e) if e.when == timeout.when => {}
            _ => return None,
        }

        self.remove_slab(timeout.slab_idx).map(|e| e.data)
    }

    fn remove_slab(&mut self, slab_idx: usize) -> Option<Entry<T>> {
        debug!("removing timer slab {}", slab_idx);
        let entry = match self.slab.remove(slab_idx) {
            Some(e) => e,
            None => return None,
        };

        // Remove the node from the linked list
        if entry.prev == EMPTY {
            self.wheel[entry.wheel_idx].head = entry.next;
        } else {
            self.slab[entry.prev].next = entry.next;
        }
        if entry.next != EMPTY {
            self.slab[entry.next].prev = entry.prev;
        }

        if self.cur_slab_idx == slab_idx {
            self.cur_slab_idx = entry.next;
        }

        return Some(entry)
    }

    fn time_to_ticks(&self, time: Instant) -> u64 {
        let dur = time - self.start;
        let ms = dur.subsec_nanos() as u64 / 1_000_000;
        let ms = dur.as_secs()
                    .checked_mul(1_000)
                    .and_then(|m| m.checked_add(ms))
                    .expect("overflow scheduling timeout");
        (ms + TICK_MS / 2) / TICK_MS
    }

    fn ticks_to_wheel_idx(&self, ticks: u64) -> usize {
        (ticks as usize) & MASK
    }
}

#[cfg(test)]
mod tests {
    extern crate env_logger;

    use std::time::{Instant, Duration};

    use super::TimerWheel;

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

    #[test]
    fn remove_then_poll() {
        drop(env_logger::init());
        let mut timer = TimerWheel::<i32>::new();
        let now = Instant::now();

        let t = timer.insert(now + ms(1), 3);
        timer.cancel(&t).unwrap();
        assert_eq!(timer.poll(now + ms(200)), None);
    }

    #[test]
    fn add_two_then_remove() {
        drop(env_logger::init());
        let mut timer = TimerWheel::<i32>::new();
        let now = Instant::now();

        let t1 = timer.insert(now + ms(1), 1);
        timer.insert(now + ms(2), 2);
        assert_eq!(timer.poll(now + ms(200)), Some(2));
        timer.cancel(&t1).unwrap();
        assert_eq!(timer.poll(now + ms(200)), None);
    }

    #[test]
    fn poll_then_next_timeout() {
        drop(env_logger::init());
        let mut timer = TimerWheel::<i32>::new();
        let now = Instant::now();

        timer.insert(now + ms(200), 2);
        assert_eq!(timer.poll(now + ms(100)), None);
        assert_eq!(timer.next_timeout(), Some(now + ms(200)));
    }

    #[test]
    fn add_remove_next_timeout() {
        drop(env_logger::init());
        let mut timer = TimerWheel::<i32>::new();
        let now = Instant::now();

        let t = timer.insert(now + ms(200), 2);
        assert_eq!(timer.cancel(&t), Some(2));
        if let Some(t) = timer.next_timeout() {
            assert_eq!(timer.poll(t + ms(100)), None);
            assert_eq!(timer.next_timeout(), None);
        }
    }
}
