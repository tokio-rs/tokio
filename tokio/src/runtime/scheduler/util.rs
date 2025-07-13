cfg_rt_and_time! {
    pub(crate) mod time {
        use crate::runtime::{scheduler::driver};
        use crate::runtime::time::{EntryHandle, Wheel};
        use std::time::Duration;
        use std::sync::mpsc;

        pub(crate) fn insert_inject_timers(
            wheel: &mut Wheel,
            tx: mpsc::Sender<EntryHandle>,
            inject: Vec<EntryHandle>,
        ) -> bool {
            let mut fired = false;
            // process injected timers
            for hdl in inject {
                unsafe {
                    if !wheel.insert(hdl.clone(), tx.clone()) {
                        // timer is already elapsed, wake it up
                        hdl.wake_unregistered();
                        fired = true;
                    }
                }
            }

            fired
        }

        pub(crate) fn remove_cancelled_timers(
            wheel: &mut Wheel,
            rx: &mpsc::Receiver<EntryHandle>,
        ) {
            while let Ok(hdl) = rx.try_recv() {
                unsafe {
                    let is_registered = hdl.is_registered();
                    let is_pending = hdl.is_pending();
                    if is_registered && !is_pending {
                        wheel.remove(hdl);
                    }
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
        ) {
            drv_hdl.with_time(|maybe_time_hdl| {
                let Some(time_hdl) = maybe_time_hdl else {
                    // time driver is not enabled, nothing to do.
                    return;
                };

                let clock = drv_hdl.clock();
                let time_source = time_hdl.time_source();

                let now = time_source.now(clock);
                time_hdl.process_at_time(wheel, now);
            });
        }

        pub(crate) fn shutdown_local_timers(
            wheel: &mut Wheel,
            tx: mpsc::Sender<EntryHandle>,
            rx: &mpsc::Receiver<EntryHandle>,
            inject: Vec<EntryHandle>,
            drv_hdl: &driver::Handle,
        ) {
            drv_hdl.with_time(|maybe_time_hdl| {
                let Some(time_hdl) = maybe_time_hdl else {
                    // time driver is not enabled, nothing to do.
                    return;
                };

                remove_cancelled_timers(wheel, rx);
                insert_inject_timers(wheel, tx, inject);
                time_hdl.shutdown(wheel);
            });
        }
    }
}
