mod expend;
use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn tokio_test(args: TokenStream, item: TokenStream) -> TokenStream {
    expend::tokio_test(args.into(), syn::parse_macro_input!(item)).into()
}
