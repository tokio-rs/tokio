use std::io::{BufRead, Read, Seek, Write};
use tokio::io::{
    AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncSeek, AsyncSeekExt, AsyncWrite,
    AsyncWriteExt,
};

/// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
/// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
#[derive(Debug)]
pub struct SyncIoBridge<T> {
    src: T,
    rt: tokio::runtime::Handle,
}

impl<T: AsyncBufRead + Unpin> BufRead for SyncIoBridge<T> {
    fn fill_buf(&mut self) -> std::io::Result<&[u8]> {
        let src = &mut self.src;
        self.rt.block_on(AsyncBufReadExt::fill_buf(src))
    }

    fn consume(&mut self, amt: usize) {
        let src = &mut self.src;
        AsyncBufReadExt::consume(src, amt)
    }

    fn read_until(&mut self, byte: u8, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt
            .block_on(AsyncBufReadExt::read_until(src, byte, buf))
    }
    fn read_line(&mut self, buf: &mut String) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(AsyncBufReadExt::read_line(src, buf))
    }
}

impl<T: AsyncRead + Unpin> Read for SyncIoBridge<T> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(AsyncReadExt::read(src, buf))
    }

    fn read_to_end(&mut self, buf: &mut Vec<u8>) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.read_to_end(buf))
    }

    fn read_to_string(&mut self, buf: &mut String) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.read_to_string(buf))
    }

    fn read_exact(&mut self, buf: &mut [u8]) -> std::io::Result<()> {
        let src = &mut self.src;
        // The AsyncRead trait returns the count, synchronous doesn't.
        let _n = self.rt.block_on(src.read_exact(buf))?;
        Ok(())
    }
}

impl<T: AsyncWrite + Unpin> Write for SyncIoBridge<T> {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.write(buf))
    }

    fn flush(&mut self) -> std::io::Result<()> {
        let src = &mut self.src;
        self.rt.block_on(src.flush())
    }

    fn write_all(&mut self, buf: &[u8]) -> std::io::Result<()> {
        let src = &mut self.src;
        self.rt.block_on(src.write_all(buf))
    }

    fn write_vectored(&mut self, bufs: &[std::io::IoSlice<'_>]) -> std::io::Result<usize> {
        let src = &mut self.src;
        self.rt.block_on(src.write_vectored(bufs))
    }
}

impl<T: AsyncSeek + Unpin> Seek for SyncIoBridge<T> {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        let src = &mut self.src;
        self.rt.block_on(AsyncSeekExt::seek(src, pos))
    }
}

// Because https://doc.rust-lang.org/std/io/trait.Write.html#method.is_write_vectored is at the time
// of this writing still unstable, we expose this as part of a standalone method.
impl<T: AsyncWrite> SyncIoBridge<T> {
    /// Determines if the underlying [`tokio::io::AsyncWrite`] target supports efficient vectored writes.
    ///
    /// See [`tokio::io::AsyncWrite::is_write_vectored`].
    pub fn is_write_vectored(&self) -> bool {
        self.src.is_write_vectored()
    }
}

impl<T: AsyncWrite + Unpin> SyncIoBridge<T> {
    /// Shutdown this writer. This method provides a way to call the [`AsyncWriteExt::shutdown`]
    /// function of the inner [`tokio::io::AsyncWrite`] instance.
    ///
    /// # Errors
    ///
    /// This method returns the same errors as [`AsyncWriteExt::shutdown`].
    ///
    /// [`AsyncWriteExt::shutdown`]: tokio::io::AsyncWriteExt::shutdown
    pub fn shutdown(&mut self) -> std::io::Result<()> {
        let src = &mut self.src;
        self.rt.block_on(src.shutdown())
    }
}

impl<T: Unpin> SyncIoBridge<T> {
    /// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
    /// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
    ///
    /// When this struct is created, it captures a handle to the current thread's runtime with [`tokio::runtime::Handle::current`].
    /// It is hence OK to move this struct into a separate thread outside the runtime, as created
    /// by e.g. [`tokio::task::spawn_blocking`].
    ///
    /// Stated even more strongly: to make use of this bridge, you *must* move
    /// it into a separate thread outside the runtime.  The synchronous I/O will use the
    /// underlying handle to block on the backing asynchronous source, via
    /// [`tokio::runtime::Handle::block_on`].  As noted in the documentation for that
    /// function, an attempt to `block_on` from an asynchronous execution context
    /// will panic.
    ///
    /// In general, blocking call on current thread are not appropriate,as
    /// it may prevent the executor from driving other futures forward.
    /// When using SyncIoBridge, think about whether there is a better approach
    /// and understand the scenarios and proper methods for using SyncIoBridge.
    ///
    /// # Wrapping `!Unpin` types
    ///
    /// Use e.g. `SyncIoBridge::new(Box::pin(src))`.
    ///
    /// # Panics
    ///
    /// This will panic if called outside the context of a Tokio runtime.
    ///
    /// # Examples
    ///
    /// If you wish to hash some data with blake3, then you should do this.
    /// This example uses BufReader as the data stream, reads all data asynchronously
    /// with [`AsyncReadExt`]'s read_to_end, and then calculates the hash:
    ///
    /// ```rust
    /// use std::io::Result;
    /// use tokio::io::{AsyncReadExt, BufReader};
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let data = b"example";
    ///     // the input data stream example
    ///     let mut reader = BufReader::new(&data[..]);
    ///
    ///     let mut hash_data = Vec::new();
    ///     reader.read_to_end(&mut hash_data).await?;
    ///     let hash = blake3::hash(&hash_data);
    ///     println!(" hash :{}", hash.to_string());
    ///     Ok(())
    /// }
    /// ```
    ///
    /// or uses [`AsyncReadExt`]'s read to asynchronously read some bytes
    /// into a fixed-size buffer each time, looping until all data is read:
    ///
    /// ```no_run
    /// let mut data = vec![0; 64*1024];
    /// loop {
    ///     let len = reader.read(&mut data).await?;
    ///     if len == 0 { break; }
    ///     hasher.update(&data[..len]);
    /// }
    /// let hash = hasher.finalize();
    /// ```
    ///
    /// This example demonstrates how [`SyncIoBridge`] converts an asynchronous data stream
    /// into synchronous reading, using [`tokio::runtime::Handle::block_on`] internally to block and read the data.
    /// you should do not this:
    ///
    /// ```no_run
    /// task::spawn_blocking(move || {
    ///     let hasher = blake3::Hasher::new();
    ///     let reader = SyncIoBridge::new(reader);
    ///     std::io::copy(&mut reader, &mut hasher);
    ///     let hash = hasher.finalize();
    /// })
    /// ```
    ///
    /// In the three examples above, the first two involve asynchronously reading data within the current runtime context.
    /// The third example uses [`SyncIoBridge`] in spawn_blocking to convert to synchronous I/O,
    /// essentially using handle::block_on. spawn_blocking creates a new operating system thread for the task.
    /// If you read very few bytes of I/O data each time and the operation is very quick,
    /// asynchronous reading is a better approach, as it avoids the overhead associated with `spawn_blocking` and `SyncIoBridge`.
    ///
    /// Other similar examples
    ///
    /// How to compress a stream of data correctly.
    /// (use async-compression and don't pass a SyncIoBridge to a non-async compression library):
    ///
    /// ```rust
    /// use std::io::Result;
    /// use async_compression::tokio::write::ZlibEncoder;
    /// use tokio::io::{AsyncWriteExt, BufReader, BufWriter}; // for `write_all` and `shutdown`
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let data = b"example";
    ///     // the input data stream example
    ///     let mut reader = BufReader::new(&data[..]);
    ///     // the output data stream
    ///     let writer = BufWriter::new(Vec::new());
    ///
    ///     let mut encoder = ZlibEncoder::new(writer);
    ///     tokio::io::copy(&mut reader, &mut encoder).await?;
    ///     encoder.shutdown().await?;
    ///
    ///     // encode data
    ///     let _encoded_data = encoder.get_ref().buffer();
    ///     Ok(())
    /// }
    /// ```
    ///
    /// How to parse data using serde-json correctly.
    /// (read data into a `Vec<u8>` and use from_slice instead of attempting to use from_reader with SyncIoBridge):
    ///
    /// ```rust
    /// use serde::{Deserialize, Serialize};
    /// use std::io::Result;
    /// use tokio::io::{AsyncReadExt, BufReader};
    ///
    /// #[derive(Serialize, Deserialize,Debug)]
    /// struct User {
    ///     name: String,
    ///     age: u8,
    /// }
    ///
    /// #[tokio::main]
    /// async fn main() -> Result<()> {
    ///     let data = b"
    ///         {
    ///             \"name\": \"Tom\",
    ///             \"age\": 23
    ///         }";
    ///     // the input data stream example
    ///     let mut reader = BufReader::new(&data[..]);
    ///
    ///     let mut json_data = Vec::new();
    ///     reader.read_to_end(&mut json_data).await?;
    ///
    ///     let u: User = serde_json::from_slice(&json_data).unwrap();
    ///     println!("{:#?}", u);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// When doing things with files, you probably want to use [`std::fs`] inside [`tokio::task::spawn_blocking`]
    /// instead of combining [`tokio::fs::File`] with [`SyncIoBridge`].
    /// Since spawn_blocking only synchronously executes a task and does not schedule multiple tasks,
    /// and handle::block_on cannot drive I/O or timers, it must wait for other threads in the runtime to handle I/O.
    /// Therefore, using tokio::fs::File for file handling within spawn_blocking is inefficient:
    ///
    /// ```rust
    /// use tokio::task;
    /// use std::fs;
    /// use std::io::{self, Read, Write};
    ///
    /// #[tokio::main]
    /// async fn main() -> io::Result<()> {
    ///     let path = "example.txt";
    ///
    ///     // write file
    ///     task::spawn_blocking(move || {
    ///         let mut file = fs::File::create(path)?;
    ///         file.write_all(b"Hello, world!")?;
    ///         Ok::<(), io::Error>(())
    ///     }).await??;
    ///
    ///     // read file
    ///     let content = task::spawn_blocking(move || {
    ///         let mut file = fs::File::open(path)?;
    ///         let mut contents = String::new();
    ///         file.read_to_string(&mut contents)?;
    ///         Ok::<_, io::Error>(contents)
    ///     }).await??;
    ///
    ///     println!("file content: {}", content);
    ///     Ok(())
    /// }
    /// ```
    ///
    /// [`tokio::fs`]: https://docs.rs/tokio/latest/tokio/fs/index.html
    /// [`tokio::fs::File`]: https://docs.rs/tokio/latest/tokio/fs/struct.File.html
    #[track_caller]
    pub fn new(src: T) -> Self {
        Self::new_with_handle(src, tokio::runtime::Handle::current())
    }

    /// Use a [`tokio::io::AsyncRead`] synchronously as a [`std::io::Read`] or
    /// a [`tokio::io::AsyncWrite`] as a [`std::io::Write`].
    ///
    /// This is the same as [`SyncIoBridge::new`], but allows passing an arbitrary handle and hence may
    /// be initially invoked outside of an asynchronous context.
    pub fn new_with_handle(src: T, rt: tokio::runtime::Handle) -> Self {
        Self { src, rt }
    }

    /// Consume this bridge, returning the underlying stream.
    pub fn into_inner(self) -> T {
        self.src
    }
}
