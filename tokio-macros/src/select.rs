use proc_macro::{TokenStream, TokenTree};
use proc_macro2::Span;
use quote::quote;
use syn::Ident;

pub(crate) fn declare_output_enum(input: TokenStream) -> TokenStream {
    // passed in is: `(_ _ _)` with one `_` per branch
    let branches = match input.into_iter().next() {
        Some(TokenTree::Group(group)) => group.stream().into_iter().count(),
        _ => panic!("unexpected macro input"),
    };

    let variants = (0..branches)
        .map(|num| Ident::new(&format!("_{}", num), Span::call_site()))
        .collect::<Vec<_>>();

    // Use a bitfield to track which futures completed
    let mask = Ident::new(
        if branches <= 8 {
            "u8"
        } else if branches <= 16 {
            "u16"
        } else if branches <= 32 {
            "u32"
        } else if branches <= 64 {
            "u64"
        } else {
            panic!("up to 64 branches supported");
        },
        Span::call_site(),
    );

    TokenStream::from(quote! {
        pub(super) enum Out<#( #variants ),*> {
            #( #variants(#variants), )*
            // Include a `Disabled` variant signifying that all select branches
            // failed to resolve.
            Disabled,
        }

        pub(super) type Mask = #mask;
    })
}
