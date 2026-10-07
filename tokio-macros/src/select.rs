use proc_macro::{TokenStream, TokenTree};
use proc_macro2::Span;
use quote::quote;
use syn::{parse::Parser, Ident};

pub(crate) fn declare_output_enum(input: TokenStream) -> TokenStream {
    // passed in is: `(_ _ _)` with one `_` per branch
    let branches = match input.into_iter().next() {
        Some(TokenTree::Group(group)) => group.stream().into_iter().count(),
        _ => panic!("unexpected macro input"),
    };

    let variants = (0..branches)
        .map(|num| Ident::new(&format!("_{num}"), Span::call_site()))
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

pub(crate) fn clean_pattern_macro(input: TokenStream) -> TokenStream {
    // If this isn't a pattern, we return the token stream as-is. The select!
    // macro is using it in a location requiring a pattern, so an error will be
    // emitted there.
    let mut input: syn::Pat = match syn::Pat::parse_multi_with_leading_vert.parse(input.clone()) {
        Ok(it) => it,
        Err(_) => return input,
    };

    clean_pattern(&mut input, false);
    quote::ToTokens::into_token_stream(input).into()
}

// Removes binding modifiers that would move or mutably borrow the output.
fn clean_pattern(pat: &mut syn::Pat, under_reference: bool) {
    match pat {
        syn::Pat::Lit(_literal) => {}
        syn::Pat::Macro(_macro) => {}
        syn::Pat::Path(_path) => {}
        syn::Pat::Range(_range) => {}
        syn::Pat::Rest(_rest) => {}
        syn::Pat::Verbatim(_tokens) => {}
        syn::Pat::Wild(_underscore) => {}
        syn::Pat::Ident(ident) => {
            // Reference patterns reset the default binding mode, so keep an
            // explicit `ref` below them to avoid moving non-Copy values.
            if !under_reference {
                ident.by_ref = None;
            }
            ident.mutability = None;
            if let Some((_at, pat)) = &mut ident.subpat {
                clean_pattern(&mut *pat, under_reference);
            }
        }
        syn::Pat::Or(or) => {
            for case in &mut or.cases {
                clean_pattern(case, under_reference);
            }
        }
        syn::Pat::Paren(paren) => {
            clean_pattern(&mut paren.pat, under_reference);
        }
        syn::Pat::Slice(slice) => {
            for elem in &mut slice.elems {
                clean_pattern(elem, under_reference);
            }
        }
        syn::Pat::Struct(struct_pat) => {
            for field in &mut struct_pat.fields {
                clean_pattern(&mut field.pat, under_reference);
            }
        }
        syn::Pat::Tuple(tuple) => {
            for elem in &mut tuple.elems {
                clean_pattern(elem, under_reference);
            }
        }
        syn::Pat::TupleStruct(tuple) => {
            for elem in &mut tuple.elems {
                clean_pattern(elem, under_reference);
            }
        }
        syn::Pat::Reference(reference) => {
            // The generated match borrows its output. Use the macro's edition
            // for reference patterns so the caller's edition does not reject
            // them under the inherited reference binding mode.
            reference.and_token.span = Span::mixed_site().located_at(reference.and_token.span);
            clean_pattern(&mut reference.pat, true);
        }
        syn::Pat::Type(type_pat) => {
            clean_pattern(&mut type_pat.pat, under_reference);
        }
        _ => {}
    }
}
