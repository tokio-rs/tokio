//! Windows specific network types.

mod named_pipe;
pub use self::named_pipe::{
    NamedPipe, NamedPipeClientOptions, NamedPipeOptions, PipeEnd, PipeMode, PipePeekInfo,
};
