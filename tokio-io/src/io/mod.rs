//! I/O conveniences when working with primitives in `tokio-core`
//!
//! Contains various combinators to work with I/O objects and type definitions
//! as well.
//!
//! A description of the high-level I/O combinators can be [found online] in
//! addition to a description of the [low level details].
//!
//! [found online]: https://tokio.rs/docs/getting-started/core/
//! [low level details]: https://tokio.rs/docs/going-deeper-tokio/core-low-level/

mod read;
mod read_exact;
mod read_to_end;
mod read_until;

pub use allow_std::AllowStdIo;
pub use copy::{copy, Copy};
pub use flush::{flush, Flush};
pub use lines::{lines, Lines};
pub use self::read::{read, Read};
pub use self::read_exact::{read_exact, ReadExact};
pub use self::read_to_end::{read_to_end, ReadToEnd};
pub use self::read_until::{read_until, ReadUntil};
pub use shutdown::{shutdown, Shutdown};
pub use split::{ReadHalf, WriteHalf};
pub use window::Window;
pub use write_all::{write_all, WriteAll};
