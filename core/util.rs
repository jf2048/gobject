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
    T: darling::FromMeta + Default
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

#[inline]
pub(crate) fn push_error<T: std::fmt::Display>(
    errors: &mut Vec<darling::Error>,
    span: proc_macro2::Span,
    message: T
) {
    errors.push(syn::Error::new(span, message).into());
}

#[inline]
pub(crate) fn push_error_spanned<T: quote::ToTokens, U: std::fmt::Display>(
    errors: &mut Vec<darling::Error>,
    tokens: T,
    message: U
) {
    errors.push(syn::Error::new_spanned(tokens, message).into());
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

#[inline]
pub(crate) fn make_attrs(tokens: TokenStream) -> Vec<syn::Attribute> {
    struct OuterAttrs(Vec<syn::Attribute>);
    impl Parse for OuterAttrs {
        fn parse(input: ParseStream) -> syn::Result<Self> {
            Ok(Self(syn::Attribute::parse_outer(input)?))
        }
    }
    parse::<OuterAttrs>(tokens, &mut vec![]).unwrap().0
}

