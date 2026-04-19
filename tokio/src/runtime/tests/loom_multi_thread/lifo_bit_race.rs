//! Loom tests for the `any_lifo` push-suppression protocol introduced in
//! commit `04353af` on PR #8069. Asks: with the four `SeqCst` ops the patch
//! defines (`Idle::put_lifo`, `Idle::clear_lifo`, `Idle::should_attempt_lifo_steal`,
//! and the LIFO cell's `swap`/`load` via `AtomicNotified`), can the cell end
//! up holding a task while the bit is cleared and no `notify` happened?
//!
//! `lifo_bit_race_pre_set` reproduces the conjectured race: when the bit is
//! pre-set by a prior pusher in the same wave, loom finds an interleaving
//! where W's push and B's `steal_stranded_lifo` interleave such that B reads
//! W's cell as empty (legal under SC ordering across distinct memory
//! locations), B clears the bit, and W's `put_lifo` reads `prev=true` so W
//! does not notify. The two control tests rule out a buggy harness:
//! `control_no_preset_is_safe` (bit starts cleared so W is forced to notify)
//! and `control_always_notify_is_safe` (W always notifies regardless of
//! `put_lifo`'s return).

use crate::runtime::scheduler::multi_thread::idle::Idle;
use crate::runtime::scheduler::multi_thread::queue;
use crate::runtime::tests::{unowned, NoopSchedule};

use loom::sync::atomic::AtomicUsize;
use loom::sync::Arc as LoomArc;
use loom::thread;
use std::sync::atomic::Ordering::SeqCst;

/// Inputs to `run`: which variant of the protocol to exercise.
struct Setup {
    /// Call `put_lifo` once before launching W and B, modeling "an earlier
    /// pusher in the same wave already set the bit". This is the precondition
    /// that distinguishes `lifo_bit_race_pre_set` from `control_no_preset_is_safe`.
    pre_set_bit: bool,
    /// Make W ignore `put_lifo`'s return value and always notify (the
    /// pre-PR-7431 / always-notify policy). Used by `control_always_notify_is_safe`.
    w_always_notify: bool,
}

/// Outputs of `run`: post-state flags the caller asserts the invariant against.
struct Outcome {
    /// W's LIFO cell still holds the task (i.e. nobody popped it).
    cell_has_task: bool,
    /// The `ANY_LIFO` bit is still set (so a future `steal_stranded_lifo`
    /// will look at peer cells again).
    bit_still_set: bool,
    /// W decided to notify when it pushed.
    w_notified: bool,
    /// B's scan observed a non-empty LIFO cell on a peer (so its
    /// `steal_stranded_lifo` would unpark a worker in the real code).
    b_found_in_scan: bool,
}

impl Outcome {
    /// True iff there is no remaining mechanism to discover the task: the
    /// cell holds work, the bit is cleared, no notify happened, and B didn't
    /// see the task either.
    fn is_stranded(&self) -> bool {
        self.cell_has_task && !self.w_notified && !self.bit_still_set && !self.b_found_in_scan
    }
}

/// Run thread `W` (push to own empty LIFO cell) and `B`
/// (`steal_stranded_lifo`) concurrently, then return the post-state.
fn run(setup: Setup) -> Outcome {
    let (idle, _synced) = Idle::new(2);
    let idle = LoomArc::new(idle);

    if setup.pre_set_bit {
        let _ = idle.put_lifo();
    }

    let (steal, local) = queue::local::<NoopSchedule>();

    let did_notify = LoomArc::new(AtomicUsize::new(0));
    let b_found = LoomArc::new(AtomicUsize::new(0));

    // W's closure returns its `Local` so the main thread can drain the LIFO
    // cell after the invariant check (Local's Drop asserts the slot is empty).
    let th_w = {
        let idle = idle.clone();
        let did_notify = did_notify.clone();
        let w_always_notify = setup.w_always_notify;
        thread::spawn(move || {
            let (task, _handle) = unowned(async {});
            let prev = local.push_lifo(task);
            assert!(prev.is_none(), "cell was supposed to start empty");
            let bit_was_set = idle.put_lifo();
            let should_notify = w_always_notify || !bit_was_set;
            if should_notify {
                did_notify.store(1, SeqCst);
            }
            local
        })
    };

    let th_b = {
        let idle = idle.clone();
        let b_found = b_found.clone();
        let steal = steal.clone();
        thread::spawn(move || {
            if !idle.should_attempt_lifo_steal() {
                return;
            }
            if steal.has_lifo() {
                b_found.store(1, SeqCst);
            } else {
                idle.clear_lifo();
            }
        })
    };

    let local = th_w.join().unwrap();
    th_b.join().unwrap();

    let outcome = Outcome {
        cell_has_task: steal.has_lifo(),
        bit_still_set: idle.should_attempt_lifo_steal(),
        w_notified: did_notify.load(SeqCst) == 1,
        b_found_in_scan: b_found.load(SeqCst) == 1,
    };

    // Drain the LIFO cell so `Local::drop`'s emptiness assertion succeeds
    // regardless of whether the race fired in this iteration.
    let _ = local.pop_lifo();

    outcome
}

/// FAIL = race confirmed: the cell ends up holding a task with no
/// remaining mechanism to discover it.
#[test]
fn lifo_bit_race_pre_set() {
    loom::model(|| {
        let outcome = run(Setup {
            pre_set_bit: true,
            w_always_notify: false,
        });
        assert!(
            !outcome.is_stranded(),
            "STRANDED: cell holds task but bit was cleared, W did not notify, \
             and B did not see the task in its scan"
        );
    });
}

/// Control: bit starts cleared, so W is the first pusher and is forced to
/// notify. The invariant must hold under every interleaving.
#[test]
fn control_no_preset_is_safe() {
    loom::model(|| {
        let outcome = run(Setup {
            pre_set_bit: false,
            w_always_notify: false,
        });
        assert!(
            !outcome.is_stranded(),
            "no-preset control violated: this should be unreachable when bit starts cleared"
        );
    });
}

/// Control: W ignores `put_lifo`'s return and always notifies. The
/// invariant must hold under every interleaving.
#[test]
fn control_always_notify_is_safe() {
    loom::model(|| {
        let outcome = run(Setup {
            pre_set_bit: true,
            w_always_notify: true,
        });
        assert!(
            !outcome.is_stranded(),
            "always-notify control violated: model is broken"
        );
    });
}
