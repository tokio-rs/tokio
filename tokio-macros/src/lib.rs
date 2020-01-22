#![doc(html_root_url = "https://docs.rs/tokio-macros/0.2.3")]
#![allow(clippy::needless_doctest_main)]
#![warn(
    missing_debug_implementations,
    missing_docs,
    rust_2018_idioms,
    unreachable_pub
)]
#![deny(intra_doc_link_resolution_failure)]
#![doc(test(
    no_crate_inject,
    attr(deny(warnings, rust_2018_idioms), allow(dead_code, unused_variables))
))]

//! Macros for use with Tokio

extern crate proc_macro;

mod entry;
mod select;

use proc_macro::TokenStream;

/// Marks async function to be executed by selected runtime.
///
/// ## Options:
///
/// - `core_threads=n` - Sets core threads to `n`.
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
/// ### Set number of core threads
///
/// ```rust
/// #[tokio::main(core_threads = 1)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main_threaded(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, true)
}

/// Marks async function to be executed by selected runtime.
///
/// ## Options:
///
/// - `basic_scheduler` - All tasks are executed on the current thread.
/// - `threaded_scheduler` - Uses the multi-threaded scheduler. Used by default.
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
/// ### Select runtime
///
/// ```rust
/// #[tokio::main(basic_scheduler)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::old::main(args, item)
}

/// Marks async function to be executed by selected runtime.
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
#[proc_macro_attribute]
#[cfg(not(test))] // Work around for rust-lang/rust#62127
pub fn main_basic(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::main(args, item, false)
}

/// Marks async function to be executed by runtime, suitable to test enviornment
///
/// ## Options:
///
/// - `basic_scheduler` - All tasks are executed on the current thread. Used by default.
/// - `threaded_scheduler` - Use multi-threaded scheduler.
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
#[proc_macro_attribute]
pub fn test_threaded(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::test(args, item, true)
}

/// Marks async function to be executed by runtime, suitable to test enviornment
///
/// ## Options:
///
/// - `core_threads=n` - Sets core threads to `n`.
/// - `max_threads=n` - Sets max threads to `n`.
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
#[proc_macro_attribute]
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    entry::old::test(args, item)
}

/// Marks async function to be executed by runtime, suitable to test enviornment
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
