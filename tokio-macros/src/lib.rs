#![doc(html_root_url = "https://docs.rs/tokio-macros/0.2.1")]
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

use proc_macro::TokenStream;
use quote::quote;
use std::num::NonZeroUsize;

#[derive(Clone, Copy, PartialEq)]
enum Runtime {
    Basic,
    Threaded,
}

fn parse_knobs(
    input: syn::ItemFn,
    args: syn::AttributeArgs,
    is_test: bool,
    rt_threaded: bool,
) -> Result<TokenStream, syn::Error> {
    let ret = &input.sig.output;
    let name = &input.sig.ident;
    let inputs = &input.sig.inputs;
    let body = &input.block;
    let attrs = &input.attrs;
    let vis = input.vis;

    if input.sig.asyncness.is_none() {
        let msg = "the async keyword is missing from the function declaration";
        return Err(syn::Error::new_spanned(input.sig.fn_token, msg));
    }

    let mut runtime = None;
    let mut core_threads = None;
    let mut max_threads = None;

    for arg in args {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue)) => {
                let ident = namevalue.path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(namevalue, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "core_threads" => {
                        if rt_threaded {
                            match &namevalue.lit {
                                syn::Lit::Int(expr) => {
                                    let num = expr.base10_parse::<NonZeroUsize>().unwrap();
                                    if num.get() > 1 {
                                        runtime = Some(Runtime::Threaded);
                                    } else {
                                        runtime = Some(Runtime::Basic);
                                    }

                                    if let Some(v) = max_threads {
                                        if v < num {
                                            return Err(syn::Error::new_spanned(
                                                namevalue,
                                                "max_threads cannot be less than core_threads",
                                            ));
                                        }
                                    }

                                    core_threads = Some(num);
                                }
                                _ => {
                                    return Err(syn::Error::new_spanned(
                                        namevalue,
                                        "core_threads argument must be an int",
                                    ))
                                }
                            }
                        } else {
                            return Err(syn::Error::new_spanned(
                                namevalue,
                                "core_threads can only be set with rt-threaded feature flag enabled",
                            ));
                        }
                    }
                    "max_threads" => match &namevalue.lit {
                        syn::Lit::Int(expr) => {
                            let num = expr.base10_parse::<NonZeroUsize>().unwrap();

                            if let Some(v) = core_threads {
                                if num < v {
                                    return Err(syn::Error::new_spanned(
                                        namevalue,
                                        "max_threads cannot be less than core_threads",
                                    ));
                                }
                            }
                            max_threads = Some(num);
                        }
                        _ => {
                            return Err(syn::Error::new_spanned(
                                namevalue,
                                "max_threads argument must be an int",
                            ))
                        }
                    },
                    name => {
                        let msg = format!("Unknown attribute pair {} is specified; expected one of: `core_threads`, `max_threads`", name);
                        return Err(syn::Error::new_spanned(namevalue, msg));
                    }
                }
            }
            syn::NestedMeta::Meta(syn::Meta::Path(path)) => {
                let ident = path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(path, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "threaded_scheduler" => {
                        runtime = Some(runtime.unwrap_or_else(|| Runtime::Threaded))
                    }
                    "basic_scheduler" => runtime = Some(runtime.unwrap_or_else(|| Runtime::Basic)),
                    name => {
                        let msg = format!("Unknown attribute {} is specified; expected `basic_scheduler` or `threaded_scheduler`", name);
                        return Err(syn::Error::new_spanned(path, msg));
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unknown attribute inside the macro",
                ));
            }
        }
    }

    let mut rt = quote! { tokio::runtime::Builder::new().basic_scheduler() };
    if rt_threaded && (runtime == Some(Runtime::Threaded) || (runtime.is_none() && !is_test)) {
        rt = quote! { #rt.threaded_scheduler() };
    }
    if let Some(v) = core_threads.map(|v| v.get()) {
        rt = quote! { #rt.core_threads(#v) };
    }
    if let Some(v) = max_threads.map(|v| v.get()) {
        rt = quote! { #rt.max_threads(#v) };
    }

    let header = {
        if is_test {
            quote! {
                #[test]
            }
        } else {
            quote! {}
        }
    };

    let result = quote! {
        #header
        #(#attrs)*
        #vis fn #name(#inputs) #ret {
            #rt
                .enable_all()
                .build()
                .unwrap()
                .block_on(async { #body })
        }
    };

    Ok(result.into())
}

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
    main(args, item, true)
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
    main(args, item, false)
}

fn main(args: TokenStream, item: TokenStream, rt_threaded: bool) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    if input.sig.ident == "main" && !input.sig.inputs.is_empty() {
        let msg = "the main function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.inputs, msg)
            .to_compile_error()
            .into();
    }

    parse_knobs(input, args, false, rt_threaded).unwrap_or_else(|e| e.to_compile_error().into())
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
pub fn test_threaded(args: TokenStream, item: TokenStream) -> TokenStream {
    test(args, item, true)
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
    test(args, item, false)
}

fn test(args: TokenStream, item: TokenStream, rt_threaded: bool) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    for attr in &input.attrs {
        if attr.path.is_ident("test") {
            let msg = "second test attribute is supplied";
            return syn::Error::new_spanned(&attr, msg)
                .to_compile_error()
                .into();
        }
    }

    if !input.sig.inputs.is_empty() {
        let msg = "the test function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.inputs, msg)
            .to_compile_error()
            .into();
    }

    parse_knobs(input, args, true, rt_threaded).unwrap_or_else(|e| e.to_compile_error().into())
}
