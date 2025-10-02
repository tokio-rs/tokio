//! Wrappers for Tokio types that implement `Stream`.

/// Error types for the wrappers.
pub mod errors {
    cfg_sync! {
        pub use crate::wrappers::broadcast::BroadcastStreamRecvError;
    }
}

mod mpsc_bounded;
pub use mpsc_bounded::ReceiverStream;

mod mpsc_unbounded;
pub use mpsc_unbounded::UnboundedReceiverStream;

cfg_sync! {
    mod broadcast;
    pub use broadcast::BroadcastStream;

    mod watch;
    pub use watch::WatchStream;
}

cfg_signal! {
    #[cfg(all(unix, not(loom)))]
    mod signal_unix;
    #[cfg(all(unix, not(loom)))]
    pub use signal_unix::SignalStream;

    #[cfg(any(windows, docsrs))]
    mod signal_windows;
    #[cfg(any(windows, docsrs))]
    pub use signal_windows::{CtrlCStream, CtrlBreakStream};
}

cfg_time! {
    mod interval;
    pub use interval::IntervalStream;
}

cfg_net! {
    #[cfg(not(loom))]
    mod tcp_listener;
    #[cfg(not(loom))]
    pub use tcp_listener::TcpListenerStream;

    #[cfg(all(unix, not(loom)))]
    mod unix_listener;
    #[cfg(all(unix, not(loom)))]
    pub use unix_listener::UnixListenerStream;
}

cfg_io_util! {
    mod split;
    pub use split::SplitStream;

    mod lines;
    pub use lines::LinesStream;
}

cfg_fs! {
    #[cfg(not(loom))]
    mod read_dir;
    #[cfg(not(loom))]
    pub use read_dir::ReadDirStream;
}
