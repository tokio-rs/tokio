mod async_buf_read_ext;
mod async_read_ext;
mod async_write_ext;
mod buf_reader;
mod buf_writer;
mod chain;
mod copy;
mod flush;
mod lines;
mod read;
mod read_exact;
mod read_line;
mod read_to_end;
mod read_to_string;
mod read_until;
mod shutdown;
mod take;
mod write;
mod write_all;

#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_buf_read_ext::AsyncBufReadExt;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_read_ext::AsyncReadExt;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_write_ext::AsyncWriteExt;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::buf_reader::BufReader;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::buf_writer::BufWriter;

// used by `BufReader` and `BufWriter`
// https://github.com/rust-lang/rust/blob/master/src/libstd/sys_common/io.rs#L1
const DEFAULT_BUF_SIZE: usize = 8 * 1024;
