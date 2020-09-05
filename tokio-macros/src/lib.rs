#![doc(html_root_url = "https://docs.rs/tokio-macros/0.3.0")]
#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![cfg_attr(docsrs, deny(broken_intra_doc_links))]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! Macros for use with Tokio

// This `extern` is required for older `rustc` versions but newer `rustc`
// versions warn about the unused `extern crate`.
#[allow(unused_extern_crates)]
extern crate proc_macro;

mod entry;
mod select;

use proc_macro::TokenStream;

/// Marks async function to be executed by selected runtime. This macro helps set up a `Runtime`
/// without requiring the user to use [Runtime](../tokio/runtime/struct.Runtime.html) or
/// [Builder](../tokio/runtime/struct.Builder.html) directly.
///
/// Note: This macro is designed to be simplistic and targets applications that do not require
/// a complex setup. If provided functionality is not sufficient, user may be interested in
/// using [Builder](../tokio/runtime/struct.Builder.html), which provides a more powerful
/// interface.
///
/// ## Options:
///
/// If you want to set the number of worker threads used for asynchronous code, use the
/// `core_threads` option.
///
/// - `core_threads=n` - Sets core threads to `n` (requires `rt-threaded` feature).
/// - `max_threads=n` - Sets max threads to `n` (requires `rt-core` or `rt-threaded` feature).
/// - `basic_scheduler` - Use the basic schduler (requires `rt-core`).
///
/// ## Function arguments:
///
/// Arguments are allowed for any functions aside from `main` which is special
///
/// ## Usage
///
/// ### Using default
///
/// ```rust
/// #[tokio::main]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new()
///         .threaded_scheduler()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Using basic scheduler
///
/// The basic scheduler is single-threaded.
///
/// ```rust
/// #[tokio::main(basic_scheduler)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new()
///         .basic_scheduler()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Set number of core threads
///
/// ```rust
/// #[tokio::main(core_threads = 2)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new()
///         .threaded_scheduler()
///         .core_threads(2)
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### NOTE:
///
/// If you rename the tokio crate in your dependencies this macro
/// will not work. If you must rename the 0.2 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.2 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main_threaded(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, true)
}

/// Marks async function to be executed by selected runtime. This macro helps set up a `Runtime`
/// without requiring the user to use [Runtime](../tokio/runtime/struct.Runtime.html) or
/// [Builder](../tokio/runtime/struct.Builder.html) directly.
///
/// Note: This macro is designed to be simplistic and targets applications that do not require
/// a complex setup. If provided functionality is not sufficient, user may be interested in
/// using [Builder](../tokio/runtime/struct.Builder.html), which provides a more powerful
/// interface.
///
/// ## Options:
///
/// - `basic_scheduler` - All tasks are executed on the current thread.
/// - `threaded_scheduler` - Uses the multi-threaded scheduler. Used by default (requires `rt-threaded` feature).
///
/// ## Function arguments:
///
/// Arguments are allowed for any functions aside from `main` which is special
///
/// ## Usage
///
/// ### Using default
///
/// ```rust
/// #[tokio::main]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Runtime::new()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Select runtime
///
/// ```rust
/// #[tokio::main(basic_scheduler)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new()
///         .basic_scheduler()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### NOTE:
///
/// If you rename the tokio crate in your dependencies this macro
/// will not work. If you must rename the 0.2 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.2 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::old::main(args, item)
}

/// Marks async function to be executed by selected runtime. This macro helps set up a `Runtime`
/// without requiring the user to use [Runtime](../tokio/runtime/struct.Runtime.html) or
/// [Builder](../tokio/runtime/struct.builder.html) directly.
///
/// ## Options:
///
/// - `max_threads=n` - Sets max threads to `n`.
///
/// ## Function arguments:
///
/// Arguments are allowed for any functions aside from `main` which is special
///
/// ## Usage
///
/// ### Using default
///
/// ```rust
/// #[tokio::main]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new()
///         .basic_scheduler()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### NOTE:
///
/// If you rename the tokio crate in your dependencies this macro
/// will not work. If you must rename the 0.2 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.2 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main_basic(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, false)
}

/// Marks async function to be executed by runtime, suitable to test environment
///
/// ## Options:
///
/// - `core_threads=n` - Sets core threads to `n` (requires `rt-threaded` feature).
/// - `max_threads=n` - Sets max threads to `n` (requires `rt-core` or `rt-threaded` feature).
///
/// ## Usage
///
/// ### Select runtime
///
/// ```no_run
/// #[tokio::test(core_threads = 1)]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// ### Using default
///
/// ```no_run
/// #[tokio::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// ### NOTE:
///
/// If you rename the tokio crate in your dependencies this macro
/// will not work. If you must rename the 0.2 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.2 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
pub fn test_threaded(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::test(args, item, true)
}

/// Marks async function to be executed by runtime, suitable to test environment
///
/// ## Options:
///
/// - `basic_scheduler` - All tasks are executed on the current thread. Used by default.
/// - `threaded_scheduler` - Use multi-threaded scheduler (requires `rt-threaded` feature).
///
/// ## Usage
///
/// ### Select runtime
///
/// ```no_run
/// #[tokio::test(threaded_scheduler)]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// ### Using default
///
/// ```no_run
/// #[tokio::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// ### NOTE:
///
/// If you rename the tokio crate in your dependencies this macro
/// will not work. If you must rename the 0.2 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.2 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::old::test(args, item)
}

/// Marks async function to be executed by runtime, suitable to test environment
///
/// ## Options:
///
/// - `max_threads=n` - Sets max threads to `n`.
///
/// ## Usage
///
/// ```no_run
/// #[tokio::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// ### NOTE:
///
/// If you rename the tokio crate in your dependencies this macro
/// will not work. If you must rename the 0.2 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.2 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
pub fn test_basic(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::test(args, item, false)
}

/// Implementation detail of the `select!` macro. This macro is **not** intended
/// to be used as part of the public API and is permitted to change.
#[proc_macro]
#[doc(hidden)]
pub fn select_priv_declare_output_enum(input: TokenStream) -> TokenStream {
    select::declare_output_enum(input)
}
