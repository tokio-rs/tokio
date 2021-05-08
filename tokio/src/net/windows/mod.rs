//! Windows specific network types.

mod named_pipe;
pub use self::named_pipe::{NamedPipe, NamedPipeBuilder, NamedPipeClientBuilder, PipeMode, PipeEnd};
