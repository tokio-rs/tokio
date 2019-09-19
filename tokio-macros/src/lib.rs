#![doc(html_root_url = "https://docs.rs/tokio-macros/0.2.0-alpha.4")]
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

/// Marks async function to be executed by selected runtime.
///
/// ## Options:
///
/// - `single_thread` - Uses `current_thread`.
/// - `multi_thread` - Uses multi-threaded runtime. Used by default.
///
/// ## Usage
///
/// ### Select runtime
///
/// ```rust
/// #[tokio::main(single_thread)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
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
pub fn main(args: TokenStream, item: TokenStream) -> TokenStream {
    enum RuntimeType {
        Single,
        Multi,
    }

    let input = syn::parse_macro_input!(item as syn::ItemFn);
    let args = syn::parse_macro_input!(args as syn::AttributeArgs);

    let ret = &input.sig.output;
    let name = &input.sig.ident;
    let body = &input.block;
    let attrs = &input.attrs;

    if input.sig.asyncness.is_none() {
        let msg = "the async keyword is missing from the function declaration";
        return syn::Error::new_spanned(input.sig.fn_token, msg)
            .to_compile_error()
            .into();
    } else if !input.sig.inputs.is_empty() {
        let msg = "the main function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.inputs, msg)
            .to_compile_error()
            .into();
    }

    let mut runtime = RuntimeType::Multi;

    for arg in args {
        if let syn::NestedMeta::Meta(syn::Meta::Path(ident)) = arg {
            let seg = ident.segments.first().expect("Must have specified ident");
            match seg.ident.to_string().to_lowercase().as_str() {
                "multi_thread" => runtime = RuntimeType::Multi,
                "single_thread" => runtime = RuntimeType::Single,
                name => {
                    let msg = format!("Unknown attribute {} is specified", name);
                    return syn::Error::new_spanned(ident, msg)
                        .to_compile_error()
                        .into();
                }
            }
        }
    }

    let result = match runtime {
        RuntimeType::Multi => quote! {
            #(#attrs)*
            fn #name() #ret {
                let mut rt = tokio::runtime::Runtime::new().unwrap();
                rt.block_on(async { #body })
            }
        },
        RuntimeType::Single => quote! {
            #(#attrs)*
            fn #name() #ret {
                let mut rt = tokio::runtime::current_thread::Runtime::new().unwrap();
                rt.block_on(async { #body })
            }
        },
    };

    result.into()
}

/// Marks async function to be executed by runtime, suitable to test enviornment
///
/// Uses `current_thread` runtime.
///
/// # Examples
///
/// ```no_run
/// #[tokio::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);

    let ret = &input.sig.output;
    let name = &input.sig.ident;
    let body = &input.block;
    let attrs = &input.attrs;

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
        return syn::Error::new_spanned(&input, msg)
            .to_compile_error()
            .into();
    } else if !input.sig.inputs.is_empty() {
        let msg = "the test function cannot accept arguments";
        return syn::Error::new_spanned(&input.sig.inputs, msg)
            .to_compile_error()
            .into();
    }

    let result = quote! {
        #[test]
        #(#attrs)*
        fn #name() #ret {
            let mut rt = tokio::runtime::current_thread::Runtime::new().unwrap();
            rt.block_on(async { #body })
        }
    };

    result.into()
}
