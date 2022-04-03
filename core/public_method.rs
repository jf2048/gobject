use crate::{util, TypeBase};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse_quote;

#[derive(Debug)]
pub struct PublicMethod {
    pub sig: syn::Signature,
}

impl PublicMethod {
    pub(crate) fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        base: TypeBase,
        errors: &mut Vec<darling::Error>,
    ) -> Vec<Self> {
        let mut public_methods = Vec::new();

        for item in items {
            if let syn::ImplItem::Method(method) = item {
                let index = method
                    .attrs
                    .iter()
                    .position(|attr| attr.path.is_ident("public"));
                if let Some(index) = index {
                    let attr = method.attrs.remove(index);
                    if !attr.tokens.is_empty() {
                        util::push_error_spanned(
                            errors,
                            &attr.tokens,
                            "Unknown tokens on public method",
                        );
                    }
                    let sig = method.sig.clone();
                    let public_method = Self { sig };
                    public_methods.push(public_method);
                }
            }
        }
        if base == TypeBase::Interface {
            for method in &public_methods {
                if let Some(recv) = method.sig.receiver() {
                    util::push_error_spanned(
                        errors,
                        recv,
                        "First argument to interface public method must be the wrapper type",
                    );
                }
            }
        }

        public_methods
    }
    fn external_sig(&self) -> syn::Signature {
        let mut sig = self.sig.clone();
        if sig.receiver().is_none() && !sig.inputs.is_empty() {
            let ref_ = util::arg_reference(&sig.inputs[0]);
            sig.inputs[0] = parse_quote! { #ref_ self };
        }
        for (index, arg) in sig.inputs.iter_mut().enumerate() {
            if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
                if !matches!(**pat, syn::Pat::Ident(_)) {
                    let ident = format_ident!("arg{}", index);
                    *pat = Box::new(parse_quote! { #ident });
                }
            }
        }
        sig
    }
    pub(crate) fn prototype(&self) -> TokenStream {
        let sig = self.external_sig();
        quote! { #sig }
    }
    pub(crate) fn definition(&self, sub_ty: &TokenStream, glib: &TokenStream) -> TokenStream {
        let proto = self.prototype();
        let ident = &self.sig.ident;
        let sig = self.external_sig();
        let args = signature_args(&sig);
        if let Some(r) = self.sig.receiver() {
            let this_ident = syn::Ident::new("____this", Span::mixed_site());
            let ref_ = util::arg_reference(r).is_none().then(|| quote! { & });
            quote! {
                #proto {
                    #![inline]
                    let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ self);
                    #sub_ty::#ident(#this_ident, #(#args),*)
                }
            }
        } else {
            let first = self.sig.inputs.first().map(|_| quote! { self, });
            quote! {
                #proto {
                    #![inline]
                    #sub_ty::#ident(#first #(#args),*)
                }
            }
        }
    }
}

#[inline]
fn signature_args<'a>(sig: &'a syn::Signature) -> impl Iterator<Item = &'a syn::Ident> + 'a {
    sig.inputs.iter().filter_map(|arg| {
        if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
            if let syn::Pat::Ident(syn::PatIdent { ident, .. }) = pat.as_ref() {
                return Some(ident);
            }
        }
        None
    })
}
