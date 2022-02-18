use proc_macro::{Ident, Spacing, Span, TokenTree};

use crate::{
    into_tokens::{braced, from_fn, group, parens, IntoTokens},
    parsing::Buf,
    token_stream::TokenStream,
};

pub(crate) fn declare_output_enum(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    // passed in is: `_ _ _` with one `_` per branch
    let branches = input.into_iter().count();

    let variants = (0..branches)
        .map(|num| Ident::new(&format!("_{}", num), Span::call_site()))
        .collect::<Vec<_>>();

    // Use a bitfield to track which futures completed
    let mask = if branches <= 8 {
        "u8"
    } else if branches <= 16 {
        "u16"
    } else if branches <= 32 {
        "u32"
    } else if branches <= 64 {
        "u64"
    } else {
        panic!("up to 64 branches supported");
    };

    let generics = from_fn(|s| {
        for variant in &variants {
            s.write((TokenTree::Ident(variant.clone()), ','));
        }
    });

    let variants = from_fn(|s| {
        for variant in &variants {
            s.write((
                TokenTree::Ident(variant.clone()),
                parens(TokenTree::Ident(variant.clone())),
                ',',
            ));
        }

        s.write(("Disabled", ','));
    });

    let out_enum = (
        ("pub", parens("super"), "enum", "Out", '<', generics, '>'),
        braced(variants),
    );

    let out = (
        out_enum,
        ("pub", parens("super"), "type", "Mask", '=', mask, ';'),
    );

    let mut stream = TokenStream::default();
    out.into_tokens(&mut stream, Span::call_site());
    stream.into_token_stream()
}

pub(crate) fn clean_pattern_macro(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let mut buf = Buf::new();

    let mut stream = TokenStream::default();
    clean_pattern(input.into_iter(), &mut buf).into_tokens(&mut stream, Span::call_site());
    stream.into_token_stream()
}

/// Clean up a pattern by skipping over any `mut` and `&` tokens.
fn clean_pattern<'a, I: 'a>(tree: I, buf: &'a mut Buf) -> impl IntoTokens + 'a
where
    I: Iterator<Item = TokenTree>,
{
    from_fn(move |s| {
        for tt in tree {
            match tt {
                TokenTree::Group(g) => {
                    s.write(group(
                        g.delimiter(),
                        clean_pattern(g.stream().into_iter(), buf),
                    ));
                }
                TokenTree::Ident(i) => {
                    if matches!(buf.display_as_str(&i), "mut" | "ref") {
                        continue;
                    }

                    s.push(TokenTree::Ident(i));
                }
                TokenTree::Punct(p) => {
                    if matches!(p.spacing(), Spacing::Alone) && p.as_char() == '&' {
                        continue;
                    }

                    s.push(TokenTree::Punct(p));
                }
                tt => {
                    s.push(tt);
                }
            }
        }
    })
}
