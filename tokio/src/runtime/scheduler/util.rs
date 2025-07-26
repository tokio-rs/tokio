cfg_rt_and_time! {
    use crate::runtime::{scheduler::driver};
    use crate::runtime::time::{EntryHandle, Wheel};
    use std::time::Duration;
    use std::sync::mpsc;

    fn insert_inject_timers(
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

    fn remove_cancelled_timers(
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

    pub(crate) fn drive_local_timers(
        wheel: &mut Wheel,
        tx: mpsc::Sender<EntryHandle>,
        rx: &mpsc::Receiver<EntryHandle>,
        inject: Vec<EntryHandle>,
        drv_hdl: &driver::Handle,
        park_duration: Option<Duration>,
    ) -> Option<Duration> {
        drv_hdl.with_time(|maybe_time_hdl| {
            let Some(time_hdl) = maybe_time_hdl else {
                // time driver is not enabled, nothing to do.
                return park_duration;
            };

            remove_cancelled_timers(wheel, rx);

            if insert_inject_timers(wheel, tx, inject) {
                return Some(Duration::from_millis(0));
            }

            let clock = drv_hdl.clock();
            let time_source = time_hdl.time_source();

            let next_timer = match wheel.next_expiration_time() {
                Some(timeout) => {
                    let now = time_source.now(clock);
                    Some(
                        time_source
                            .tick_to_duration(timeout.saturating_sub(now)),
                    )
                }
                None => None,
            };

            let park_duration = match (next_timer, park_duration) {
                (Some(next_timer), Some(park_duration)) => Some(next_timer.min(park_duration)),
                (Some(next_timer), None) => Some(next_timer),
                (None, Some(park_duration)) => Some(park_duration),
                (None, None) => None,
            };

            #[cfg(feature = "test-util")]
            if let Some(park_duration) = park_duration {
                if clock.can_auto_advance() {
                    if !time_hdl.did_wake() {
                        if let Err(msg) = clock.advance(park_duration) {
                            panic!("{msg}");
                        }
                    }

                    let now = time_source.now(clock);
                    time_hdl.process_at_time(wheel, now);
                    return Some(Duration::ZERO);
                }
            }

            let now = time_source.now(clock);
            time_hdl.process_at_time(wheel, now);
            park_duration
        })
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
