cfg_rt_and_time! {
    pub(crate) mod time {
        use crate::runtime::{scheduler::driver};
        use crate::runtime::time::{Wheel, WakeQueue};
        use crate::runtime::time::{EntryHandle, EntryState, EntryCancelling};
        use crate::runtime::time::cancellation_queue::{Sender, Receiver};
        use crate::runtime::time::EntryTransitionToWakingUp;
        use std::time::Duration;

        pub(crate) fn min_duration(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
            match (a, b) {
                (Some(dur_a), Some(dur_b)) => Some(std::cmp::min(dur_a, dur_b)),
                (Some(dur_a), None) => Some(dur_a),
                (None, Some(dur_b)) => Some(dur_b),
                (None, None) => None,
            }
        }

        pub(crate) fn insert_inject_timers(
            wheel: &mut Wheel,
            tx: &Sender,
            inject: Vec<EntryHandle>,
            wake_queue: &mut WakeQueue,
        ) {
            use crate::runtime::time::Insert;

            // process injected timers
            for hdl in inject {
                match unsafe { wheel.insert(hdl.clone(), tx.clone()) } {
                    Insert::Success => {}
                    Insert::Elapsed => {
                        match hdl.transition_to_waking_up_unregistered() {
                            EntryTransitionToWakingUp::Success => {
                                // Safety:
                                //
                                // 1. this entry is not in the timer wheel
                                // 2. AND this entry is not in any cancellation queue
                                unsafe {
                                    wake_queue.push_front(hdl);
                                }
                            }
                            EntryTransitionToWakingUp::Cancelling => {
                                // cancellation happens concurrently, no need to wake
                            }
                        }
                    }
                    Insert::Cancelling => {}
                }
            }
        }

        pub(crate) fn remove_cancelled_timers(
            wheel: &mut Wheel,
            rx: &mut Receiver,
        ) {
            for hdl in rx.recv_all() {
                match hdl.state() {
                    // INVARIANT: the state always be transitioned to Cancelling before being sent to cancellation queue
                    state @ (EntryState::Unregistered | EntryState::Registered | EntryState::Pending | EntryState::WakingUp) => {
                        panic!("corrupted state: {state:#?}");
                    }
                    EntryState::Cancelling(cancelling) => match cancelling {
                        EntryCancelling::Unregistered => (),
                        EntryCancelling::Registered | EntryCancelling::Pending => {
                            // Safety:
                            // 1. entry is either in slot or pending list
                            // 2. `rx` ensures that the entry is registered in this thread.
                            unsafe {
                                wheel.remove(hdl);
                            }
                        }
                    }
                    // INVARIANT: the state always be transitioned to Cancelling before being sent to cancellation queue
                    EntryState::WokenUp => panic!("corrupted state: `EntryState::WokenUp`"),
                }
            }
        }

        pub(crate) fn next_expiration_time(
            wheel: &Wheel,
            drv_hdl: &driver::Handle,
        ) -> Option<Duration> {
            drv_hdl.with_time(|maybe_time_hdl| {
                let Some(time_hdl) = maybe_time_hdl else {
                    // time driver is not enabled, nothing to do.
                    return None;
                };

                let clock = drv_hdl.clock();
                let time_source = time_hdl.time_source();

                wheel.next_expiration_time().map(|tick| {
                    let now = time_source.now(clock);
                    time_source.tick_to_duration(tick.saturating_sub(now))
                })
            })
        }

        cfg_test_util! {
            pub(crate) fn pre_auto_advance(
                drv_hdl: &driver::Handle,
                duration: Option<Duration>,
            ) -> bool {
                drv_hdl.with_time(|maybe_time_hdl| {
                    if maybe_time_hdl.is_none() {
                        // time driver is not enabled, nothing to do.
                        return false;
                    }

                    if duration.is_some() {
                        let clock = drv_hdl.clock();
                        if clock.can_auto_advance() {
                            return true;
                        }

                        false
                    } else {
                        false
                    }
                })
            }

            pub(crate) fn post_auto_advance(
                drv_hdl: &driver::Handle,
                duration: Option<Duration>,
            ) {
                drv_hdl.with_time(|maybe_time_hdl| {
                    let Some(time_hdl) = maybe_time_hdl else {
                        // time driver is not enabled, nothing to do.
                        return;
                    };

                    if let Some(park_duration) = duration {
                        let clock = drv_hdl.clock();
                        if clock.can_auto_advance()
                            && !time_hdl.did_wake() {
                                if let Err(msg) = clock.advance(park_duration) {
                                    panic!("{msg}");
                                }
                            }
                    }
                })
            }
        }

        cfg_not_test_util! {
            pub(crate) fn pre_auto_advance(
                _drv_hdl: &driver::Handle,
                _duration: Option<Duration>,
            ) -> bool {
                false
            }

            pub(crate) fn post_auto_advance(
                _drv_hdl: &driver::Handle,
                _duration: Option<Duration>,
            ) {
                // No-op in non-test util builds
            }
        }

        pub(crate) fn process_expired_timers(
            wheel: &mut Wheel,
            drv_hdl: &driver::Handle,
            wake_queue: &mut WakeQueue,
        ) {
            drv_hdl.with_time(|maybe_time_hdl| {
                let Some(time_hdl) = maybe_time_hdl else {
                    // time driver is not enabled, nothing to do.
                    return;
                };

                let clock = drv_hdl.clock();
                let time_source = time_hdl.time_source();

                let now = time_source.now(clock);
                time_hdl.process_at_time(wheel, now, wake_queue);
            });
        }

        pub(crate) fn shutdown_local_timers(
            wheel: &mut Wheel,
            rx: &mut Receiver,
            inject: Vec<EntryHandle>,
            drv_hdl: &driver::Handle,
        ) {
            drv_hdl.with_time(|maybe_time_hdl| {
                let Some(time_hdl) = maybe_time_hdl else {
                    // time driver is not enabled, nothing to do.
                    return;
                };

                remove_cancelled_timers(wheel, rx);
                time_hdl.shutdown(wheel);

                let mut wake_queue = WakeQueue::new();
                // simply wake all unregistered timers
                for hdl in inject {
                    match hdl.transition_to_waking_up_unregistered() {
                        EntryTransitionToWakingUp::Success => {
                            // Safety:
                            //
                            // 1. this entry is not in the timer wheel
                            // 2. AND this entry is not in any cancellation queue
                            unsafe {
                                wake_queue.push_front(hdl);
                            }
                        }
                        EntryTransitionToWakingUp::Cancelling => {
                            // cancellation happens concurrently, no need to wake
                        }
                    }
                }

                wake_queue.wake_all();
            });
        }
    }
}
