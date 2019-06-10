#![deny(missing_debug_implementations, unreachable_pub, rust_2018_idioms)]
#![cfg_attr(test, deny(warnings))]
#![doc(test(no_crate_inject, attr(deny(rust_2018_idioms))))]

//! Macros for use with Tokio

extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

/// Define the program entry point
///
/// # Examples
///
/// ```
/// #[tokio::main]
/// async fn main() {
///     println!("Hello world");
/// }
#[proc_macro_attribute]
pub fn main(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(item as syn::ItemFn);

    let ret = &input.decl.output;
    let name = &input.ident;
    let body = &input.block;

    if input.asyncness.is_none() {
        let tokens = quote_spanned! { input.span() =>
          compile_error!("the async keyword is missing from the function declaration");
        };

        return TokenStream::from(tokens);
    }

    let result = quote! {
        fn #name() #ret {
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on_async(async { #body })
        }
    };

    result.into()
}

/// Define a Tokio aware unit test
///
/// # Examples
///
/// ```
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
            rt.block_on_async(async { #body })
        }
    };

    result.into()
}
