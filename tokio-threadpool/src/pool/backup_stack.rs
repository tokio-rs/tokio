use pool::{Backup, BackupId};

use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering::{AcqRel, Acquire};

#[derive(Debug)]
pub(crate) struct BackupStack {
    state: AtomicUsize,
}

#[derive(Debug, Eq, PartialEq, Clone, Copy)]
struct State(usize);

pub(crate) const MAX_BACKUP: usize = 1 << 15;

/// Extracts the head of the backup stack from the state
const STACK_MASK: usize = ((1 << 16) - 1);

/// Used to mark the stack as empty
pub(crate) const EMPTY: BackupId = BackupId(MAX_BACKUP);

/// Used to mark the stack as terminated
pub(crate) const TERMINATED: BackupId = BackupId(EMPTY.0 + 1);

/// How many bits the Treiber ABA guard is offset by
const ABA_GUARD_SHIFT: usize = 16;

#[cfg(target_pointer_width = "64")]
const ABA_GUARD_MASK: usize = (1 << (64 - ABA_GUARD_SHIFT)) - 1;

#[cfg(target_pointer_width = "32")]
const ABA_GUARD_MASK: usize = (1 << (32 - ABA_GUARD_SHIFT)) - 1;

// ===== impl BackupStack =====

impl BackupStack {
    pub fn new() -> BackupStack {
        let state = AtomicUsize::new(State::new().into());
        BackupStack { state }
    }

    /// Push a backup thread onto the stack
    ///
    /// # Return
    ///
    /// Returns `Ok` on success.
    ///
    /// Returns `Err` if the pool has transitioned to the `TERMINATED` state.
    /// When terminated, pushing new entries is no longer permitted.
    pub fn push(&self, entries: &[Backup], id: BackupId) -> Result<(), ()> {
        let mut state: State = self.state.load(Acquire).into();

        entries[id.0].set_pushed(AcqRel);

        loop {
            let mut next = state;

            let head = state.head();

            if head == TERMINATED {
                // The pool is terminated, cannot push the sleeper.
                return Err(());
            }

            entries[id.0].set_next_sleeper(head);
            next.set_head(id);

            let actual = self
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

            if state == actual {
                return Ok(());
            }

            state = actual;
        }
    }

    /// Pop a backup thread off the stack.
    ///
    /// If `terminate` is set and the stack is empty when this function is
    /// called, the state of the stack is transitioned to "terminated". At this
    /// point, no further entries can be pushed onto the stack.
    ///
    /// # Return
    ///
    /// * Returns the index of the popped worker and the worker's observed
    ///   state.
    ///
    /// * `Ok(None)` if the stack is empty.
    /// * `Err(_)` is returned if the pool has been shutdown.
    pub fn pop(&self, entries: &[Backup], terminate: bool) -> Result<Option<BackupId>, ()> {
        // Figure out the empty value
        let terminal = match terminate {
            true => TERMINATED,
            false => EMPTY,
        };

        let mut state: State = self.state.load(Acquire).into();

        loop {
            let head = state.head();

            if head == EMPTY {
                let mut next = state;
                next.set_head(terminal);

                if next == state {
                    debug_assert!(terminal == EMPTY);
                    return Ok(None);
                }

                let actual = self
                    .state
                    .compare_and_swap(state.into(), next.into(), AcqRel)
                    .into();

                if actual != state {
                    state = actual;
                    continue;
                }

                return Ok(None);
            } else if head == TERMINATED {
                return Err(());
            }

            debug_assert!(head.0 < MAX_BACKUP);

            let mut next = state;

            let next_head = entries[head.0].next_sleeper();

            // TERMINATED can never be set as the "next pointer" on a worker.
            debug_assert!(next_head != TERMINATED);

            if next_head == EMPTY {
                next.set_head(terminal);
            } else {
                next.set_head(next_head);
            }

            let actual = self
                .state
                .compare_and_swap(state.into(), next.into(), AcqRel)
                .into();

            if actual == state {
                debug_assert!(entries[head.0].is_pushed());
                return Ok(Some(head));
            }

            state = actual;
        }
    }
}

// ===== impl State =====

impl State {
    fn new() -> State {
        State(EMPTY.0)
    }

    fn head(&self) -> BackupId {
        BackupId(self.0 & STACK_MASK)
    }

    fn set_head(&mut self, val: BackupId) {
        let val = val.0;

        // The ABA guard protects against the ABA problem w/ Treiber stacks
        let aba_guard = ((self.0 >> ABA_GUARD_SHIFT) + 1) & ABA_GUARD_MASK;

        self.0 = (aba_guard << ABA_GUARD_SHIFT) | val;
    }
}

impl From<usize> for State {
    fn from(src: usize) -> Self {
        State(src)
    }
}

impl From<State> for usize {
    fn from(src: State) -> Self {
        src.0
    }
}
