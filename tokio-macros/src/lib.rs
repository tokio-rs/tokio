extern crate proc_macro;

use proc_macro::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

/// Creates an async unit test.
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

    if input.asyncness.is_none() {
        let tokens = quote_spanned! { input.span() =>
          compile_error!("the async keyword is missing from the function declaration");
        };

        return TokenStream::from(tokens);
    }

    let result = quote! {
        #[test]
        fn #name() #ret {
            let mut rt = tokio::runtime::Runtime::new().unwrap();
            rt.block_on_async(async { #body })
        }
    };

    result.into()
}
