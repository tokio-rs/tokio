//! See [std::os](https://doc.rust-lang.org/std/os/index.html).

/// Platform-specific extensions to `std` for Windows.
///
/// See [std::os::windows](https://doc.rust-lang.org/std/os/windows/index.html).
pub mod windows {
    /// Windows-specific extensions to general I/O primitives.
    ///
    /// See [std::os::windows::io](https://doc.rust-lang.org/std/os/windows/io/index.html).
    pub mod io {
        /// See [std::os::windows::io::RawHandle](https://doc.rust-lang.org/std/os/windows/io/type.RawHandle.html)
        pub type RawHandle = crate::doc::NotDefinedHere;

        /// See [std::os::windows::io::AsRawHandle](https://doc.rust-lang.org/std/os/windows/io/trait.AsRawHandle.html)
        pub trait AsRawHandle {
            /// See [std::os::windows::io::AsRawHandle::as_raw_handle](https://doc.rust-lang.org/std/os/windows/io/trait.AsRawHandle.html#tymethod.as_raw_handle)
            fn as_raw_handle(&self) -> RawHandle;
        }

        /// See [std::os::windows::io::FromRawHandle](https://doc.rust-lang.org/std/os/windows/io/trait.FromRawHandle.html)
        pub trait FromRawHandle {
            /// See [std::os::windows::io::FromRawHandle::from_raw_handle](https://doc.rust-lang.org/std/os/windows/io/trait.FromRawHandle.html#tymethod.from_raw_handle)
            unsafe fn from_raw_handle(handle: RawHandle) -> Self;
        }

        /// See [std::os::windows::io::BorrowedHandle](https://doc.rust-lang.org/std/os/windows/io/struct.BorrowedHandle.html)
        pub type BorrowedHandle<'handle> = crate::doc::NotDefinedHere;

        /// See [std::os::windows::io::AsHandle](https://doc.rust-lang.org/std/os/windows/io/trait.AsHandle.html)
        pub trait AsHandle {
            /// See [std::os::windows::io::AsHandle::as_handle](https://doc.rust-lang.org/std/os/windows/io/trait.AsHandle.html#tymethod.as_handle)
            fn as_handle(&self) -> BorrowedHandle<'_>;
        }
    }
}
