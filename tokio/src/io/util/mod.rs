mod async_buf_read_ext;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_buf_read_ext::AsyncBufReadExt;

mod async_read_ext;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_read_ext::AsyncReadExt;

mod async_write_ext;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::async_write_ext::AsyncWriteExt;

mod buf_reader;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::buf_reader::BufReader;

mod buf_stream;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::buf_stream::BufStream;

mod buf_writer;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::buf_writer::BufWriter;

mod chain;

mod copy;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::copy::{copy, Copy};

mod empty;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::empty::{empty, Empty};

mod flush;

mod lines;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::lines::Lines;

mod read;
mod read_exact;
mod read_line;
mod read_to_end;
mod read_to_string;
mod read_until;

mod repeat;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::repeat::{repeat, Repeat};

mod shutdown;

mod sink;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::sink::{sink, Sink};

mod split;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::split::Split;

mod take;
#[allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411
pub use self::take::Take;

mod write;
mod write_all;

// used by `BufReader` and `BufWriter`
// https://github.com/rust-lang/rust/blob/master/src/libstd/sys_common/io.rs#L1
const DEFAULT_BUF_SIZE: usize = 8 * 1024;
