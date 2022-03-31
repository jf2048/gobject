use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};

#[derive(Debug)]
pub struct PublicMethod {
    pub sig: syn::Signature,
}

impl PublicMethod {
    pub(crate) fn many_from_items(items: &mut Vec<syn::ImplItem>) -> Vec<Self> {
        let mut public_methods = Vec::new();

        for item in items {
            if let syn::ImplItem::Method(method) = item {
                if matches!(method.vis, syn::Visibility::Public(_)) {
                    method.vis = syn::parse_quote! { pub(super) };
                    let sig = method.sig.clone();
                    let public_method = Self { sig };
                    public_methods.push(public_method);
                }
            }
        }

        public_methods
    }
    fn external_sig(&self) -> syn::Signature {
        let mut sig = self.sig.clone();
        if sig.receiver().is_some() && sig.inputs.len() > 1 {
            let mut inputs = sig.inputs.into_iter().collect::<Vec<_>>();
            inputs.remove(1);
            sig.inputs = FromIterator::from_iter(inputs.into_iter());
        }
        for (index, arg) in sig.inputs.iter_mut().enumerate() {
            if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
                if !matches!(**pat, syn::Pat::Ident(_)) {
                    *pat = Box::new(syn::Pat::Ident(syn::PatIdent {
                        attrs: Default::default(),
                        by_ref: None,
                        mutability: None,
                        ident: format_ident!("arg{}", index),
                        subpat: None,
                    }));
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
        let args = signature_args(&sig).collect::<Vec<_>>();
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        if let Some(r) = sig.receiver() {
            let is_ref = match r {
                syn::FnArg::Receiver(r) => r.reference.is_some(),
                syn::FnArg::Typed(t) => matches!(*t.ty, syn::Type::Reference(_)),
            };
            let self_ = match is_ref {
                true => quote! { self },
                false => quote! { &self },
            };
            let wrapper = if self.sig.inputs.len() > 1 {
                Some(quote! { #self_, })
            } else {
                None
            };
            quote_spanned! { Span::mixed_site() =>
                #proto {
                    #![inline]
                    let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#self_);
                    #sub_ty::#ident(#this_ident, #wrapper #(#args),*)
                }
            }
        } else {
            quote_spanned! { Span::mixed_site() =>
                #proto {
                    #![inline]
                    #sub_ty::#ident(#(#args),*)
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
