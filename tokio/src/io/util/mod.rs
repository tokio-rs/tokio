#![allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411

cfg_io_util! {
    mod async_buf_read_ext;
    pub use async_buf_read_ext::AsyncBufReadExt;

    mod buf_reader;
    pub use buf_reader::BufReader;

    mod buf_stream;
    pub use buf_stream::BufStream;

    mod buf_writer;
    pub use buf_writer::BufWriter;

    mod copy;
    pub use copy::copy;

    mod copy_buf;
    pub use copy_buf::copy_buf;

    mod empty;
    pub use empty::{empty, Empty};

    mod lines;
    pub use lines::Lines;

    mod mem;
    pub use mem::{duplex, DuplexStream};

    mod repeat;
    pub use repeat::{repeat, Repeat};

    mod sink;
    pub use sink::{sink, Sink};

    mod split;
    pub use split::Split;

    // used by `BufReader` and `BufWriter`
    // https://github.com/rust-lang/rust/blob/master/src/libstd/sys_common/io.rs#L1
    const DEFAULT_BUF_SIZE: usize = 8 * 1024;
}

cfg_not_io_util! {
    cfg_process! {
        mod read_to_end;
        // Used by process
        pub(crate) use read_to_end::read_to_end;
    }
}
