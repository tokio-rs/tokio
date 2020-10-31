#![allow(unreachable_pub)] // https://github.com/rust-lang/rust/issues/57411

cfg_io_util! {
    mod async_vectored_write_ext;
    pub use async_vectored_write_ext::AsyncVectoredWriteExt;

    mod write_vectored;
    pub use write_vectored::WriteVectored;
}
