//! See [`std::os`](https://doc.rust-lang.org/std/os/index.html).

/// Platform-specific extensions to `std` for Windows.
///
/// See [`std::os::windows`](https://doc.rust-lang.org/std/os/windows/index.html).
pub mod windows {
    /// Windows-specific extensions to general I/O primitives.
    ///
    /// See [`std::os::windows::io`](https://doc.rust-lang.org/std/os/windows/io/index.html).
    pub mod io {
        /// See [`std::os::windows::io::RawHandle`](https://doc.rust-lang.org/std/os/windows/io/type.RawHandle.html)
        pub type RawHandle = crate::doc::NotDefinedHere;

        /// See [`std::os::windows::io::OwnedHandle`](https://doc.rust-lang.org/std/os/windows/io/struct.OwnedHandle.html)
        pub type OwnedHandle = crate::doc::NotDefinedHere;

        /// See [`std::os::windows::io::AsRawHandle`](https://doc.rust-lang.org/std/os/windows/io/trait.AsRawHandle.html)
        pub trait AsRawHandle {
            /// See [`std::os::windows::io::AsRawHandle::as_raw_handle`](https://doc.rust-lang.org/std/os/windows/io/trait.AsRawHandle.html#tymethod.as_raw_handle)
            fn as_raw_handle(&self) -> RawHandle;
        }

        /// See [`std::os::windows::io::FromRawHandle`](https://doc.rust-lang.org/std/os/windows/io/trait.FromRawHandle.html)
        pub trait FromRawHandle {
            /// See [`std::os::windows::io::FromRawHandle::from_raw_handle`](https://doc.rust-lang.org/std/os/windows/io/trait.FromRawHandle.html#tymethod.from_raw_handle)
            unsafe fn from_raw_handle(handle: RawHandle) -> Self;
        }

        /// See [`std::os::windows::io::IntoRawHandle`](https://doc.rust-lang.org/std/os/windows/io/trait.IntoRawHandle.html)
        pub trait IntoRawHandle {
            /// See [`std::os::windows::io::IntoRawHandle::into_raw_handle`](https://doc.rust-lang.org/std/os/windows/io/trait.IntoRawHandle.html#tymethod.into_raw_handle)
            fn into_raw_handle(self) -> RawHandle;
        }

        /// See [`std::os::windows::io::RawSocket`](https://doc.rust-lang.org/std/os/windows/io/type.RawSocket.html)
        pub type RawSocket = crate::doc::NotDefinedHere;

        /// See [`std::os::windows::io::OwnedSocket`](https://doc.rust-lang.org/std/os/windows/io/struct.OwnedSocket.html)
        pub type OwnedSocket = crate::doc::NotDefinedHere;

        /// See [`std::os::windows::io::AsRawSocket`](https://doc.rust-lang.org/std/os/windows/io/trait.AsRawSocket.html)
        pub trait AsRawSocket {
            /// See [`std::os::windows::io::AsRawSocket::as_raw_socket`](https://doc.rust-lang.org/std/os/windows/io/trait.AsRawSocket.html#tymethod.as_raw_socket)
            fn as_raw_socket(&self) -> RawSocket;
        }

        /// See [`std::os::windows::io::FromRawSocket`](https://doc.rust-lang.org/std/os/windows/io/trait.FromRawSocket.html)
        pub trait FromRawSocket {
            /// See [`std::os::windows::io::FromRawSocket::from_raw_socket`](https://doc.rust-lang.org/std/os/windows/io/trait.FromRawSocket.html#tymethod.from_raw_socket)
            unsafe fn from_raw_socket(sock: RawSocket) -> Self;
        }

        /// See [`std::os::windows::io::IntoRawSocket`](https://doc.rust-lang.org/std/os/windows/io/trait.IntoRawSocket.html)
        pub trait IntoRawSocket {
            /// See [`std::os::windows::io::IntoRawSocket::into_raw_socket`](https://doc.rust-lang.org/std/os/windows/io/trait.IntoRawSocket.html#tymethod.into_raw_socket)
            fn into_raw_socket(self) -> RawSocket;
        }

        /// See [`std::os::windows::io::BorrowedHandle`](https://doc.rust-lang.org/std/os/windows/io/struct.BorrowedHandle.html)
        pub type BorrowedHandle<'handle> = crate::doc::NotDefinedHere;

        /// See [`std::os::windows::io::AsHandle`](https://doc.rust-lang.org/std/os/windows/io/trait.AsHandle.html)
        pub trait AsHandle {
            /// See [`std::os::windows::io::AsHandle::as_handle`](https://doc.rust-lang.org/std/os/windows/io/trait.AsHandle.html#tymethod.as_handle)
            fn as_handle(&self) -> BorrowedHandle<'_>;
        }

        /// See [`std::os::windows::io::BorrowedSocket`](https://doc.rust-lang.org/std/os/windows/io/struct.BorrowedSocket.html)
        pub type BorrowedSocket<'socket> = crate::doc::NotDefinedHere;

        /// See [`std::os::windows::io::AsSocket`](https://doc.rust-lang.org/std/os/windows/io/trait.AsSocket.html)
        pub trait AsSocket {
            /// See [`std::os::windows::io::AsSocket::as_socket`](https://doc.rust-lang.org/std/os/windows/io/trait.AsSocket.html#tymethod.as_socket)
            fn as_socket(&self) -> BorrowedSocket<'_>;
        }
    }

    /// Windows-specific extensions to `std::ffi`.
    ///
    /// See [`std::os::windows::ffi`](https://doc.rust-lang.org/std/os/windows/ffi/index.html).
    pub mod ffi {
        /// See [`std::os::windows::ffi::OsStrExt`](https://doc.rust-lang.org/std/os/windows/ffi/trait.OsStrExt.html)
        pub trait OsStrExt {
            /// See [`std::os::windows::ffi::OsStrExt::encode_wide`](https://doc.rust-lang.org/std/os/windows/ffi/trait.OsStrExt.html#tymethod.encode_wide)
            fn encode_wide(&self) -> std::iter::Empty<u16>;
        }

        impl OsStrExt for std::ffi::OsStr {
            fn encode_wide(&self) -> std::iter::Empty<u16> {
                std::iter::empty()
            }
        }
    }

    /// Windows-specific extensions to `std::fs`.
    ///
    /// See [`std::os::windows::fs`](https://doc.rust-lang.org/std/os/windows/fs/index.html).
    #[cfg(feature = "fs")]
    pub mod fs {
        /// See [`std::os::windows::fs::OpenOptionsExt`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html)
        pub trait OpenOptionsExt {
            /// See [`std::os::windows::fs::OpenOptionsExt::access_mode`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.access_mode)
            fn access_mode(&mut self, access: u32) -> &mut Self;

            /// See [`std::os::windows::fs::OpenOptionsExt::share_mode`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.share_mode)
            fn share_mode(&mut self, val: u32) -> &mut Self;

            /// See [`std::os::windows::fs::OpenOptionsExt::custom_flags`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.custom_flags)
            fn custom_flags(&mut self, flags: u32) -> &mut Self;

            /// See [`std::os::windows::fs::OpenOptionsExt::attributes`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.attributes)
            fn attributes(&mut self, attributes: u32) -> &mut Self;

            /// See [`std::os::windows::fs::OpenOptionsExt::security_qos_flags`](https://doc.rust-lang.org/std/os/windows/fs/trait.OpenOptionsExt.html#tymethod.security_qos_flags)
            fn security_qos_flags(&mut self, flags: u32) -> &mut Self;
        }

        impl OpenOptionsExt for std::fs::OpenOptions {
            fn access_mode(&mut self, _access: u32) -> &mut Self {
                self
            }

            fn share_mode(&mut self, _val: u32) -> &mut Self {
                self
            }

            fn custom_flags(&mut self, _flags: u32) -> &mut Self {
                self
            }

            fn attributes(&mut self, _attributes: u32) -> &mut Self {
                self
            }

            fn security_qos_flags(&mut self, _flags: u32) -> &mut Self {
                self
            }
        }
    }

    /// Windows-specific extensions to `std::process`.
    ///
    /// See [`std::os::windows::process`](https://doc.rust-lang.org/std/os/windows/process/index.html).
    #[cfg(feature = "process")]
    pub mod process {
        /// See [`std::os::windows::process::CommandExt`](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html)
        pub trait CommandExt {
            /// See [`std::os::windows::process::CommandExt::raw_arg`](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.raw_arg)
            fn raw_arg<S: AsRef<std::ffi::OsStr>>(&mut self, text_to_append_as_is: S) -> &mut Self;

            /// See [`std::os::windows::process::CommandExt::creation_flags`](https://doc.rust-lang.org/std/os/windows/process/trait.CommandExt.html#tymethod.creation_flags)
            fn creation_flags(&mut self, flags: u32) -> &mut Self;
        }

        impl CommandExt for std::process::Command {
            fn raw_arg<S: AsRef<std::ffi::OsStr>>(
                &mut self,
                _text_to_append_as_is: S,
            ) -> &mut Self {
                self
            }

            fn creation_flags(&mut self, _flags: u32) -> &mut Self {
                self
            }
        }
    }

    use self::io::{
        AsRawHandle, AsRawSocket, FromRawHandle, FromRawSocket, IntoRawSocket, RawHandle, RawSocket,
    };

    // The `std` and `mio`/`socket2` types below are documented by Tokio's
    // Windows-only code but are themselves platform specific. Implementing the
    // shim traits for them keeps those bodies type checkable on Unix
    // documentation builds; see `crate::doc::NotDefinedHere`.

    impl AsRawHandle for std::io::Stdin {
        fn as_raw_handle(&self) -> RawHandle {
            unreachable!()
        }
    }

    impl AsRawHandle for std::io::Stdout {
        fn as_raw_handle(&self) -> RawHandle {
            unreachable!()
        }
    }

    impl AsRawHandle for std::io::Stderr {
        fn as_raw_handle(&self) -> RawHandle {
            unreachable!()
        }
    }

    #[cfg(feature = "fs")]
    impl AsRawHandle for std::fs::File {
        fn as_raw_handle(&self) -> RawHandle {
            unreachable!()
        }
    }

    #[cfg(feature = "fs")]
    impl FromRawHandle for std::fs::File {
        unsafe fn from_raw_handle(_handle: RawHandle) -> Self {
            unreachable!()
        }
    }

    #[cfg(feature = "fs")]
    impl From<crate::doc::NotDefinedHere> for std::fs::File {
        fn from(_handle: crate::doc::NotDefinedHere) -> Self {
            unreachable!()
        }
    }

    #[cfg(feature = "net")]
    impl AsRawSocket for mio::net::TcpListener {
        fn as_raw_socket(&self) -> RawSocket {
            unreachable!()
        }
    }

    #[cfg(feature = "net")]
    impl AsRawSocket for mio::net::TcpStream {
        fn as_raw_socket(&self) -> RawSocket {
            unreachable!()
        }
    }

    #[cfg(feature = "net")]
    impl AsRawSocket for mio::net::UdpSocket {
        fn as_raw_socket(&self) -> RawSocket {
            unreachable!()
        }
    }

    #[cfg(feature = "net")]
    impl AsRawSocket for socket2::Socket {
        fn as_raw_socket(&self) -> RawSocket {
            unreachable!()
        }
    }

    #[cfg(feature = "net")]
    impl IntoRawSocket for socket2::Socket {
        fn into_raw_socket(self) -> RawSocket {
            unreachable!()
        }
    }

    #[cfg(feature = "net")]
    impl FromRawSocket for socket2::Socket {
        unsafe fn from_raw_socket(_sock: RawSocket) -> Self {
            unreachable!()
        }
    }

    #[cfg(feature = "process")]
    impl AsRawHandle for crate::process::unix::Child {
        fn as_raw_handle(&self) -> RawHandle {
            unreachable!()
        }
    }
}
