#![allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411

cfg_io_util! {
    mod async_buf_read_ext;
    pub use async_buf_read_ext::AsyncBufReadExt;

    mod async_read_ext;
    pub use async_read_ext::AsyncReadExt;

    mod async_seek_ext;
    pub use async_seek_ext::AsyncSeekExt;

    mod async_write_ext;
    pub use async_write_ext::AsyncWriteExt;

    mod buf_reader;
    pub use buf_reader::BufReader;

    mod buf_stream;
    pub use buf_stream::BufStream;

    mod buf_writer;
    pub use buf_writer::BufWriter;

    mod chain;

    mod copy;
    pub use copy::{copy, Copy};

    mod empty;
    pub use empty::{empty, Empty};

    mod flush;

    mod lines;
    pub use lines::Lines;

    mod read;
    mod read_buf;
    mod read_exact;
    mod read_int;
    mod read_line;

    mod read_to_end;
    cfg_process! {
        pub(crate) use read_to_end::read_to_end;
    }

    mod read_to_string;
    mod read_until;

    mod repeat;
    pub use repeat::{repeat, Repeat};

    mod shutdown;

    mod sink;
    pub use sink::{sink, Sink};

    mod split;
    pub use split::Split;

    cfg_stream! {
        mod stream_reader;
        pub use stream_reader::{stream_reader, StreamReader};
    }

    mod take;
    pub use take::Take;

    mod write;
    mod write_all;
    mod write_buf;
    mod write_int;


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
