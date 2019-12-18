#![doc(html_root_url = "https://docs.rs/tokio-macros/0.2.1")]
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

enum Runtime {
    Basic,
    Threaded,
    Auto,
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
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    let ret = &input.sig.output;
    let name = &input.sig.ident;
    let inputs = &input.sig.inputs;
    let body = &input.block;
    let attrs = &input.attrs;
    let vis = input.vis;

    if input.sig.asyncness.is_none() {
        let msg = "the async keyword is missing from the function declaration";
        return syn::Error::new_spanned(input.sig.fn_token, msg)
            .to_compile_error()
            .into();
    } else if name == "main" && !inputs.is_empty() {
        let msg = "the main function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.inputs, msg)
            .to_compile_error()
            .into();
    }

    let mut runtime = Runtime::Auto;

    for arg in args {
        if let syn::NestedMeta::Meta(syn::Meta::Path(path)) = arg {
            let ident = path.get_ident();
            if ident.is_none() {
                let msg = "Must have specified ident";
                return syn::Error::new_spanned(path, msg).to_compile_error().into();
            }
            match ident.unwrap().to_string().to_lowercase().as_str() {
                "threaded_scheduler" => runtime = Runtime::Threaded,
                "basic_scheduler" => runtime = Runtime::Basic,
                name => {
                    let msg = format!("Unknown attribute {} is specified; expected `basic_scheduler` or `threaded_scheduler`", name);
                    return syn::Error::new_spanned(path, msg).to_compile_error().into();
                }
            }
        }
    }

    let result = match runtime {
        Runtime::Threaded | Runtime::Auto => quote! {
            #(#attrs)*
            #vis fn #name(#inputs) #ret {
                tokio::runtime::Runtime::new().unwrap().block_on(async { #body })
            }
        },
        Runtime::Basic => quote! {
            #(#attrs)*
            #vis fn #name(#inputs) #ret {
                tokio::runtime::Builder::new()
                    .basic_scheduler()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async { #body })
            }
        },
    };

    result.into()
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
pub fn test(args: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    let ret = &input.sig.output;
    let name = &input.sig.ident;
    let body = &input.block;
    let attrs = &input.attrs;
    let vis = input.vis;

    for attr in attrs {
        if attr.path.is_ident("test") {
            let msg = "second test attribute is supplied";
            return syn::Error::new_spanned(&attr, msg)
                .to_compile_error()
                .into();
        }
    }

    if input.sig.asyncness.is_none() {
        let msg = "the async keyword is missing from the function declaration";
        return syn::Error::new_spanned(&input.sig.fn_token, msg)
            .to_compile_error()
            .into();
    } else if !input.sig.inputs.is_empty() {
        let msg = "the test function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.inputs, msg)
            .to_compile_error()
            .into();
    }

    let mut runtime = Runtime::Auto;

    for arg in args {
        if let syn::NestedMeta::Meta(syn::Meta::Path(path)) = arg {
            let ident = path.get_ident();
            if ident.is_none() {
                let msg = "Must have specified ident";
                return syn::Error::new_spanned(path, msg).to_compile_error().into();
            }
            match ident.unwrap().to_string().to_lowercase().as_str() {
                "threaded_scheduler" => runtime = Runtime::Threaded,
                "basic_scheduler" => runtime = Runtime::Basic,
                name => {
                    let msg = format!("Unknown attribute {} is specified; expected `basic_scheduler` or `threaded_scheduler`", name);
                    return syn::Error::new_spanned(path, msg).to_compile_error().into();
                }
            }
        }
    }

    let result = match runtime {
        Runtime::Threaded => quote! {
            #[test]
            #(#attrs)*
            #vis fn #name() #ret {
                tokio::runtime::Runtime::new().unwrap().block_on(async { #body })
            }
        },
        Runtime::Basic | Runtime::Auto => quote! {
            #[test]
            #(#attrs)*
            #vis fn #name() #ret {
                tokio::runtime::Builder::new()
                    .basic_scheduler()
                    .enable_all()
                    .build()
                    .unwrap()
                    .block_on(async { #body })
            }
        },
    };

    result.into()
}
