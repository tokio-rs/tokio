//! Wrappers for Tokio types that implement `Stream`.

mod mpsc_bounded;
pub use mpsc_bounded::ReceiverStream;

mod mpsc_unbounded;
pub use mpsc_unbounded::UnboundedReceiverStream;

cfg_time! {
    mod interval;
    pub use interval::IntervalStream;
}

cfg_net! {
    mod tcp_listener;
    pub use tcp_listener::TcpListenerStream;

    #[cfg(unix)]
    mod unix_listener;
    #[cfg(unix)]
    pub use unix_listener::UnixListenerStream;
}

cfg_io_util! {
    mod split;
    pub use split::SplitStream;

    mod lines;
    pub use lines::LinesStream;
}

cfg_fs! {
    mod read_dir;
    pub use read_dir::ReadDirStream;
}
