use heck::ToKebabCase;
use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream, Parser};

pub fn parse<T: Parse>(input: TokenStream, errors: &mut Vec<darling::Error>) -> Option<T> {
    match <T as Parse>::parse.parse2(input) {
        Ok(t) => Some(t),
        Err(e) => {
            errors.push(e.into());
            None
        }
    }
}

#[derive(Default)]
struct AttributeArgs(syn::AttributeArgs);

impl Parse for AttributeArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let mut metas = Vec::new();

        loop {
            if input.is_empty() {
                break;
            }
            let value = input.parse()?;
            metas.push(value);
            if input.is_empty() {
                break;
            }
            input.parse::<syn::Token![,]>()?;
        }

        Ok(Self(metas))
    }
}

pub(crate) fn parse_list<T>(input: TokenStream, errors: &mut Vec<darling::Error>) -> T
where
    T: darling::FromMeta + Default,
{
    let args = parse::<AttributeArgs>(input, errors).unwrap_or_default().0;
    match T::from_list(&args) {
        Ok(args) => args,
        Err(e) => {
            errors.push(e);
            Default::default()
        }
    }
}

#[derive(Default)]
struct ParenAttributeArgs(syn::AttributeArgs);

impl Parse for ParenAttributeArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let args = if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            content.parse::<AttributeArgs>()?.0
        } else {
            Default::default()
        };
        input.parse::<syn::parse::Nothing>()?;
        Ok(Self(args))
    }
}

pub(crate) fn parse_paren_list<T>(input: TokenStream, errors: &mut Vec<darling::Error>) -> T
where
    T: darling::FromMeta + Default,
{
    let args = parse::<ParenAttributeArgs>(input, errors)
        .unwrap_or_default()
        .0;
    match T::from_list(&args) {
        Ok(args) => args,
        Err(e) => {
            errors.push(e);
            Default::default()
        }
    }
}

#[inline]
pub(crate) fn push_error<T: std::fmt::Display>(
    errors: &mut Vec<darling::Error>,
    span: proc_macro2::Span,
    message: T,
) {
    errors.push(syn::Error::new(span, message).into());
}

#[inline]
pub(crate) fn push_error_spanned<T: quote::ToTokens, U: std::fmt::Display>(
    errors: &mut Vec<darling::Error>,
    tokens: T,
    message: U,
) {
    errors.push(syn::Error::new_spanned(tokens, message).into());
}

pub(crate) fn format_name(ident: &syn::Ident) -> String {
    let ident = ident.to_string();
    let mut s = ident.as_str();
    while let Some(n) = s.strip_prefix('_') {
        s = n;
    }
    s.to_kebab_case()
}

pub(crate) fn is_valid_name(name: &str) -> bool {
    let mut iter = name.chars();
    if let Some(c) = iter.next() {
        if !c.is_ascii_alphabetic() {
            return false;
        }
        for c in iter {
            if !c.is_ascii_alphanumeric() && c != '-' && c != '_' {
                return false;
            }
        }
        true
    } else {
        false
    }
}

#[inline]
pub(crate) fn make_stmt(tokens: TokenStream) -> TokenStream {
    quote::quote! { #tokens; }
}
