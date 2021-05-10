//! Windows specific network types.

mod named_pipe;
pub use self::named_pipe::{
    wait_named_pipe, NamedPipe, NamedPipeClientOptions, NamedPipeOptions, PipeEnd, PipeMode,
};
