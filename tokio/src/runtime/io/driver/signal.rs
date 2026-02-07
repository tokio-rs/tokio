use super::{Driver, Handle, TOKEN_SIGNAL};

use std::io;

impl Handle {
    pub(crate) fn register_signal_receiver(
        &self,
        receiver: &mut mio::net::UnixStream,
    ) -> io::Result<()> {
        self.registry
            .register(receiver, TOKEN_SIGNAL, mio::Interest::READABLE)?;
        // Note: we intentionally do NOT increment io_event_sources here.
        // The signal receiver is an internal mechanism (like the mio::Waker)
        // and should not prevent the poll-skip optimization in turn().
        // Signal readiness is dispatched via the signal_ready flag, which
        // is set when TOKEN_SIGNAL events arrive during an actual poll.
        Ok(())
    }
}

impl Driver {
    pub(crate) fn consume_signal_ready(&mut self) -> bool {
        let ret = self.signal_ready;
        self.signal_ready = false;
        ret
    }
}
