#![deny(missing_debug_implementations, unreachable_pub, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! Macros for use with Tokio

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

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
///#![feature(async_await)]
///
/// #[tokio::main(single_thread)]
/// async fn main() {
///     println!("Hello world");
/// }
/// ```
/// ### Using default
///
/// ```rust
///#![feature(async_await)]
///
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

    let ret = &input.decl.output;
    let name = &input.ident;
    let body = &input.block;
    let attrs = &input.attrs;

    if input.asyncness.is_none() {
        let tokens = quote_spanned! { input.span() =>
          compile_error!("the async keyword is missing from the function declaration");
        };

        return TokenStream::from(tokens);
    }

    let mut runtime = RuntimeType::Multi;

    for arg in args {
        if let syn::NestedMeta::Meta(syn::Meta::Word(ident)) = arg {
            match ident.to_string().to_lowercase().as_str() {
                "multi_thread" => runtime = RuntimeType::Multi,
                "single_thread" => runtime = RuntimeType::Single,
                name => panic!("Unknown attribute {} is specified", name),
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
/// ```ignore
/// #![feature(async_await)]
///
/// #[tokio::test]
/// async fn my_test() {
///     assert!(true);
/// }
/// ```
#[proc_macro_attribute]
pub fn test(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);

    let ret = &input.decl.output;
    let name = &input.ident;
    let body = &input.block;
    let attrs = &input.attrs;

    for attr in attrs {
        if attr.path.is_ident("test") {
            let tokens = quote_spanned! { input.span() =>
                compile_error!("second test attribute is supplied");
            };

            return TokenStream::from(tokens);
        }
    }

    if input.asyncness.is_none() {
        let tokens = quote_spanned! { input.span() =>
          compile_error!("the async keyword is missing from the function declaration");
        };

        return TokenStream::from(tokens);
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
