use crate::util;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{
    parse::Parser, parse_macro_input, DeriveInput, Ident, ItemFn, ReturnType, Signature, Type,
};

#[derive(Default)]
struct ResourceMeta {
    concrete_type: Option<String>,
    resource_kind: Option<String>,
}

const RESOURCE_SPAN_NAME: &str = "resource";
const OP_SPAN_NAME: &str = "resource_op";
const RESULT_TYPE_FIELD_NAME: &str = "result_type";
const OP_FIELD_NAME: &str = "op";

const RESULT_TYPE_READY: &str = "READY";
const RESULT_TYPE_PENDING: &str = "PENDING";
const RESULT_TYPE_ERROR: &str = "ERROR";

pub(crate) fn add_resource_impl(args: TokenStream, input: TokenStream) -> TokenStream {
    let mut ast = parse_macro_input!(input as DeriveInput);

    // add the span field
    match &mut ast.data {
        syn::Data::Struct(ref mut struct_data) => match &mut struct_data.fields {
            syn::Fields::Named(fields) => {
                fields.named.push(
                    syn::Field::parse_named
                        .parse2(quote! { span: tracing::Span })
                        .unwrap(),
                );
            }
            _ => {
                let message = "Only structs with named fields are supported";
                return syn::Error::new(Span::call_site(), message)
                    .to_compile_error()
                    .into();
            }
        },
        _ => {
            let message = "Only structs are supported";
            return syn::Error::new(Span::call_site(), message)
                .to_compile_error()
                .into();
        }
    }

    // add getter impl
    let args = parse_macro_input!(args as syn::AttributeArgs);

    let name = &ast.ident;
    let resource_meta = match parse_resource_impld_args(args) {
        Ok(meta) => meta,
        Err(e) => return e.to_compile_error().into(),
    };

    let concrete_type = match resource_meta.concrete_type {
        Some(t) => t,
        None => name.to_string(),
    };

    let resource_kind = match resource_meta.resource_kind {
        Some(kind) => kind,
        None => {
            let message = "Unspecified resource_kind";
            return syn::Error::new(Span::call_site(), message)
                .to_compile_error()
                .into();
        }
    };

    let gen = quote! {
        #ast
        impl #name {
            fn create_span() -> tracing::Span {
                tracing::trace_span!(#RESOURCE_SPAN_NAME, concrete_type = #concrete_type, kind = #resource_kind)
            }
        }
    };

    gen.into()
}

fn parse_resource_impld_args(args: syn::AttributeArgs) -> Result<ResourceMeta, syn::Error> {
    let mut resource_meta = ResourceMeta::default();
    for arg in args {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue)) => {
                let ident = namevalue.path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(namevalue, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "concrete_type" => {
                        let concrete_type = util::parse_string(
                            namevalue.lit.clone(),
                            syn::spanned::Spanned::span(&namevalue.lit),
                            "concrete_type",
                        )?;

                        resource_meta.concrete_type = Some(concrete_type);
                    }
                    "resource_kind" => {
                        let resource_kind = util::parse_string(
                            namevalue.lit.clone(),
                            syn::spanned::Spanned::span(&namevalue.lit),
                            "resource_kind",
                        )?;

                        resource_meta.resource_kind = Some(resource_kind);
                    }
                    name => {
                        let msg = format!(
                            "Unknown attribute {} is specified; expected one of: `concrete_type`, `resource_kind`",
                            name,
                        );
                        return Err(syn::Error::new_spanned(namevalue, msg));
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unsupported attribute inside the macro",
                ));
            }
        }
    }

    Ok(resource_meta)
}

pub(crate) fn instrument_resource_op(args: TokenStream, input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as ItemFn);

    let ItemFn {
        attrs,
        vis,
        block,
        sig,
        ..
    } = input;

    if sig.receiver().is_none() {
        let message = "Instrumented op needs to be a method on a Resource";
        return syn::Error::new_spanned(sig.fn_token, message)
            .to_compile_error()
            .into();
    }

    if sig.asyncness.is_some() {
        let message = "Cannot instrument async resource ops";
        return syn::Error::new_spanned(sig.fn_token, message)
            .to_compile_error()
            .into();
    }

    let Signature {
        output,
        inputs,
        unsafety,
        asyncness,
        constness,
        abi,
        ident,
        generics:
            syn::Generics {
                params: gen_params,
                where_clause,
                ..
            },
        ..
    } = sig;

    let returns_poll = match output.clone() {
        ReturnType::Default => false,
        ReturnType::Type(_, ty) => match ty.as_ref() {
            Type::Path(ty) => match ty.path.segments.last() {
                Some(ps) => ps.ident == "Poll",
                None => false,
            },
            _ => false,
        },
    };

    if !returns_poll {
        let message = "Instrumented op should return Poll<Result<_, _>>";
        return syn::Error::new_spanned(sig.fn_token, message)
            .to_compile_error()
            .into();
    }

    let op_name = ident.to_string();

    let op_span = quote!(tracing::trace_span!(
        #OP_SPAN_NAME,
        #OP_FIELD_NAME = #op_name,
        #RESULT_TYPE_FIELD_NAME = tracing::field::Empty,
    ));

    let args = parse_macro_input!(args as syn::AttributeArgs);

    let resource_span_guard = match parse_resource_span_location(args) {
        Ok(Some(loc)) => quote!(
            let __resource_span = self.#loc.span.clone();
            let __resource_span_guard = __resource_span.enter();
        ),
        Ok(None) => quote!(
            let __resource_span = self.span.clone();
            let __resource_span_guard = __resource_span.enter();
        ),
        Err(e) => return e.to_compile_error().into(),
    };

    let instrumented_block = quote_spanned!(block.span()=>
        #resource_span_guard
        let __op_span = #op_span;
        let __op_span_guard = __op_span.enter();
        match (move || #block)() {
            std::task::Poll::Ready(Ok(v)) => {
                __op_span.record(#RESULT_TYPE_FIELD_NAME, &#RESULT_TYPE_READY);
                drop(__op_span_guard);
                std::task::Poll::Ready(Ok(v))
            },
            std::task::Poll::Ready(Err(e)) => {
                __op_span.record(#RESULT_TYPE_FIELD_NAME, &#RESULT_TYPE_ERROR);
                drop(__op_span_guard);
                std::task::Poll::Ready(Err(e))
            },
            std::task::Poll::Pending => {
                __op_span.record(#RESULT_TYPE_FIELD_NAME, &#RESULT_TYPE_PENDING);
                drop(__op_span_guard);
                std::task::Poll::Pending
            }
        }
    );

    quote!(
        #(#attrs) *
        #vis #constness #unsafety #asyncness #abi fn #ident<#gen_params>(#inputs) #output
        #where_clause
        {
            #instrumented_block
        }
    )
    .into()
}

fn parse_resource_span_location(args: syn::AttributeArgs) -> Result<Option<Ident>, syn::Error> {
    if let Some(arg) = args.first() {
        match arg {
            syn::NestedMeta::Meta(syn::Meta::NameValue(namevalue)) => {
                let ident = namevalue.path.get_ident();
                if ident.is_none() {
                    let msg = "Must have specified ident";
                    return Err(syn::Error::new_spanned(namevalue, msg));
                }
                match ident.unwrap().to_string().to_lowercase().as_str() {
                    "resource_span_location" => {
                        let resource_span_location = util::parse_string(
                            namevalue.lit.clone(),
                            syn::spanned::Spanned::span(&namevalue.lit),
                            "resource_span_location",
                        )?;

                        return Ok(Some(Ident::new(&resource_span_location, Span::call_site())));
                    }
                    name => {
                        let msg = format!(
                            "Unknown attribute {} is specified; expected: `resource_span_location`",
                            name,
                        );
                        return Err(syn::Error::new_spanned(namevalue, msg));
                    }
                }
            }
            other => {
                return Err(syn::Error::new_spanned(
                    other,
                    "Unsupported attribute inside the macro",
                ));
            }
        }
    }
    Ok(None)
}
