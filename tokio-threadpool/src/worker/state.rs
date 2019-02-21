use std::fmt;

/// Tracks worker state
#[derive(Clone, Copy, Eq, PartialEq)]
pub(crate) struct State(usize);

/// Set when the worker is pushed onto the scheduler's stack of sleeping
/// threads.
pub(crate) const PUSHED_MASK: usize = 0b001;

/// Manages the worker lifecycle part of the state
const LIFECYCLE_MASK: usize = 0b1110;
const LIFECYCLE_SHIFT: usize = 1;

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Clone, Copy)]
#[repr(usize)]
pub(crate) enum Lifecycle {
    /// The worker does not currently have an associated thread.
    Shutdown = 0 << LIFECYCLE_SHIFT,

    /// The worker is doing work
    Running = 1 << LIFECYCLE_SHIFT,

    /// The worker is currently asleep in the condvar
    Sleeping = 2 << LIFECYCLE_SHIFT,

    /// The worker has been notified it should process more work.
    Notified = 3 << LIFECYCLE_SHIFT,

    /// A stronger form of notification. In this case, the worker is expected to
    /// wakeup and try to acquire more work... if it enters this state while
    /// already busy with other work, it is expected to signal another worker.
    Signaled = 4 << LIFECYCLE_SHIFT,
}

impl State {
    /// Returns true if the worker entry is pushed in the sleeper stack
    pub fn is_pushed(&self) -> bool {
        self.0 & PUSHED_MASK == PUSHED_MASK
    }

    pub fn set_pushed(&mut self) {
        self.0 |= PUSHED_MASK
    }

    pub fn is_notified(&self) -> bool {
        use self::Lifecycle::*;

        match self.lifecycle() {
            Notified | Signaled => true,
            _ => false,
        }
    }

    pub fn lifecycle(&self) -> Lifecycle {
        Lifecycle::from(self.0 & LIFECYCLE_MASK)
    }

    pub fn set_lifecycle(&mut self, val: Lifecycle) {
        self.0 = (self.0 & !LIFECYCLE_MASK) | (val as usize)
    }

    pub fn is_signaled(&self) -> bool {
        self.lifecycle() == Lifecycle::Signaled
    }

    pub fn notify(&mut self) {
        use self::Lifecycle::Signaled;

        if self.lifecycle() != Signaled {
            self.set_lifecycle(Signaled)
        }
    }
}

impl Default for State {
    fn default() -> State {
        // All workers will start pushed in the sleeping stack
        State(PUSHED_MASK)
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

impl fmt::Debug for State {
    fn fmt(&self, fmt: &mut fmt::Formatter) -> fmt::Result {
        fmt.debug_struct("worker::State")
            .field("lifecycle", &self.lifecycle())
            .field("is_pushed", &self.is_pushed())
            .finish()
    }
}

// ===== impl Lifecycle =====

impl From<usize> for Lifecycle {
    fn from(src: usize) -> Lifecycle {
        use self::Lifecycle::*;

        debug_assert!(
            src == Shutdown as usize
                || src == Running as usize
                || src == Sleeping as usize
                || src == Notified as usize
                || src == Signaled as usize
        );

        unsafe { ::std::mem::transmute(src) }
    }
}

impl From<Lifecycle> for usize {
    fn from(src: Lifecycle) -> usize {
        let v = src as usize;
        debug_assert!(v & LIFECYCLE_MASK == v);
        v
    }
}

#[cfg(test)]
mod test {
    use super::Lifecycle::*;
    use super::*;

    #[test]
    fn lifecycle_encode() {
        let lifecycles = &[Shutdown, Running, Sleeping, Notified, Signaled];

        for &lifecycle in lifecycles {
            let mut v: usize = lifecycle.into();
            v &= LIFECYCLE_MASK;

            assert_eq!(lifecycle, Lifecycle::from(v));
        }
    }

    #[test]
    fn lifecycle_ord() {
        assert!(Running >= Shutdown);
        assert!(Signaled >= Notified);
        assert!(Signaled >= Sleeping);
    }
}
