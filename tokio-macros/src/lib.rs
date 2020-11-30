#![doc(html_root_url = "https://docs.rs/tokio-macros/0.3.1")]
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

/// Marks async function to be executed by the selected runtime. This macro helps
/// set up a `Runtime` without requiring the user to use
/// [Runtime](../tokio/runtime/struct.Runtime.html) or
/// [Builder](../tokio/runtime/struct.Builder.html) directly.
///
/// Note: This macro is designed to be simplistic and targets applications that
/// do not require a complex setup. If the provided functionality is not
/// sufficient, you may be interested in using
/// [Builder](../tokio/runtime/struct.Builder.html), which provides a more
/// powerful interface.
///
/// Note: This macro can be used on any function and not just the `main`
/// function. Using it on a non-main function makes the function behave
/// as if it was synchronous by starting a new runtime each time it is called.
/// If the function is called often, it is preferable to create the runtime using
/// the runtime builder so the runtime can be reused across calls.
///
/// # Multi-threaded runtime
///
/// To use the multi-threaded runtime, the macro can be configured using
///
/// ```
/// #[tokio::main(flavor = "multi_thread", worker_threads = 10)]
/// # async fn main() {}
/// ```
///
/// The `worker_threads` option configures the number of worker threads, and
/// defaults to the number of cpus on the system. This is the default flavor.
///
/// # Current thread runtime
///
/// To use the single-threaded runtime known as the `current_thread` runtime,
/// the macro can be configured using
///
/// ```
/// #[tokio::main(flavor = "current_thread")]
/// # async fn main() {}
/// ```
///
/// ## Function arguments:
///
/// Arguments are allowed for any functions aside from `main` which is special
///
/// ## Usage
///
/// ### Using the multi-thread runtime
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
///     tokio::runtime::Builder::new_multi_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Using current thread runtime
///
/// The basic scheduler is single-threaded.
///
/// ```rust
/// #[tokio::main(flavor = "current_thread")]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new_current_thread()
///         .enable_all()
///         .build()
///         .unwrap()
///         .block_on(async {
///             println!("Hello world");
///         })
/// }
/// ```
///
/// ### Set number of worker threads
///
/// ```rust
/// #[tokio::main(worker_threads = 2)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new_multi_thread()
///         .worker_threads(2)
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
/// If you rename the tokio crate in your dependencies this macro will not work.
/// If you must rename the 0.3 version of tokio because you're also using the
/// 0.1 version of tokio, you _must_ make the tokio 0.3 crate available as
/// `tokio` in the module where this macro is expanded.
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, true)
}

/// Marks async function to be executed by selected runtime. This macro helps set up a `Runtime`
/// without requiring the user to use [Runtime](../tokio/runtime/struct.Runtime.html) or
/// [Builder](../tokio/runtime/struct.builder.html) directly.
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
/// #[tokio::main(flavor = "current_thread")]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
///
/// Equivalent code not using `#[tokio::main]`
///
/// ```rust
/// fn main() {
///     tokio::runtime::Builder::new_current_thread()
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
/// will not work. If you must rename the 0.3 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.3 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main_rt(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, false)
}

/// Marks async function to be executed by runtime, suitable to test environment
///
/// ## Usage
///
/// ### Multi-thread runtime
///
/// ```no_run
/// #[tokio::test(flavor = "multi_thread", worker_threads = 1)]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
///
/// ### Using default
///
/// The default test runtime is single-threaded.
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
/// will not work. If you must rename the 0.3 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.3 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::test(args, item, true)
}

/// Marks async function to be executed by runtime, suitable to test environment
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
/// will not work. If you must rename the 0.3 version of tokio because
/// you're also using the 0.1 version of tokio, you _must_ make the
/// tokio 0.3 crate available as `tokio` in the module where this
/// macro is expanded.
#[proc_macro_attribute]
pub fn test_rt(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::test(args, item, false)
}

/// Always fails with the error message below.
/// ```text
/// The #[tokio::main] macro requires rt or rt-multi-thread.
/// ```
#[proc_macro_attribute]
pub fn main_fail(_args: TokenStream, _item: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "The #[tokio::main] macro requires rt or rt-multi-thread.",
    )
    .to_compile_error()
    .into()
}

/// Always fails with the error message below.
/// ```text
/// The #[tokio::test] macro requires rt or rt-multi-thread.
/// ```
#[proc_macro_attribute]
pub fn test_fail(_args: TokenStream, _item: TokenStream) -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "The #[tokio::test] macro requires rt or rt-multi-thread.",
    )
    .to_compile_error()
    .into()
}

/// Implementation detail of the `select!` macro. This macro is **not** intended
/// to be used as part of the public API and is permitted to change.
#[proc_macro]
#[doc(hidden)]
pub fn select_priv_declare_output_enum(input: TokenStream) -> TokenStream {
    select::declare_output_enum(input)
}
