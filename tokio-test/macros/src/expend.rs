use quote2::{proc_macro2::TokenStream, quote, utils::quote_rep, Quote, ToTokens};
use syn::{
    parse::{Parse, ParseStream, Parser},
    punctuated::Punctuated,
    Attribute, Meta, MetaNameValue, Signature, Token, Visibility,
};

type AttributeArgs = Punctuated<Meta, Token![,]>;

pub struct ItemFn {
    pub attrs: Vec<Attribute>,
    pub vis: Visibility,
    pub sig: Signature,
    pub body: TokenStream,
}

impl Parse for ItemFn {
    fn parse(input: ParseStream) -> Result<Self, syn::Error> {
        Ok(Self {
            attrs: input.call(Attribute::parse_outer)?,
            vis: input.parse()?,
            sig: input.parse()?,
            body: input.parse()?,
        })
    }
}

pub fn tokio_test(args: TokenStream, item_fn: ItemFn) -> TokenStream {
    let metadata = match AttributeArgs::parse_terminated.parse2(args) {
        Ok(args) => args,
        Err(err) => return err.into_compile_error(),
    };

    let has_miri_cfg = metadata.iter().any(|meta| meta.path().is_ident("miri"));
    let id_multi_thread = metadata.iter().any(|meta| match meta {
        Meta::NameValue(meta) if meta.path.is_ident("flavor") => {
            match meta.value.to_token_stream().to_string().as_str() {
                "multi_thread" => true,
                "current_thread" => false,
                val => panic!("unknown `flavor = {val}`, expected: multi_thread | current_thread"),
            }
        }
        _ => false,
    });
    let config = quote_rep(metadata, |t, meta| {
        for key in ["miri", "flavor"] {
            if meta.path().is_ident(key) {
                return;
            }
        }
        if let Meta::NameValue(MetaNameValue { path, value, .. }) = &meta {
            for key in ["worker_threads", "start_paused"] {
                if path.is_ident(key) {
                    quote!(t, { .#path(#value) });
                    return;
                }
            }
        }
        panic!("unknown config `{}`", meta.path().to_token_stream())
    });
    let runtime_type = quote(|t| {
        if id_multi_thread {
            quote!(t, { new_multi_thread });
        } else {
            quote!(t, { new_current_thread });
        }
    });
    let ignore_miri = quote(|t| {
        if !has_miri_cfg {
            quote!(t, { #[cfg_attr(miri, ignore)] });
        }
    });
    let miri_test_executor = quote(|t| {
        if has_miri_cfg {
            quote!(t, {
                if cfg!(miri) {
                    return tokio_test::task::spawn(body).block_on();
                }
            });
        }
    });

    let ItemFn {
        attrs,
        vis,
        mut sig,
        body,
    } = item_fn;

    let async_keyword = sig.asyncness.take();
    let attrs = quote_rep(attrs, |t, attr| {
        quote!(t, { #attr });
    });

    let mut out = TokenStream::new();
    quote!(out, {
        #attrs
        #ignore_miri
        #[::core::prelude::v1::test]
        #vis #sig {
            let body = #async_keyword #body;
            let body= ::std::pin::pin!(body);

            #miri_test_executor

            #[allow(clippy::expect_used, clippy::diverging_sub_expression, clippy::needless_return)]
            {
                return tokio::runtime::Builder::#runtime_type()
                    #config
                    .enable_all()
                    .build()
                    .expect("Failed building the Runtime")
                    .block_on(body);
            }
        }
    });
    out
}
