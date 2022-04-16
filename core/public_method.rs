use crate::{
    util::{self, Errors},
    TypeBase,
};
use darling::{util::Flag, FromMeta};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse_quote;

#[derive(Debug)]
pub struct PublicMethod {
    pub sig: syn::Signature,
    pub static_: bool,
}

#[derive(Default, FromMeta)]
#[darling(default)]
struct PublicMethodAttrs {
    #[darling(rename = "static")]
    static_: Flag,
}

impl PublicMethod {
    pub(crate) fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        base: TypeBase,
        errors: &Errors,
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
                    let attrs = util::parse_paren_list::<PublicMethodAttrs>(attr.tokens, errors);
                    let sig = method.sig.clone();
                    if let Some(recv) = sig.receiver() {
                        if attrs.static_.is_some() {
                            errors.push_spanned(recv, "`self` not allowed on public static method");
                        }
                        if base == TypeBase::Interface {
                            errors.push_spanned(
                                recv,
                                "First argument to interface public method must be the wrapper type",
                            );
                        }
                    }
                    let public_method = Self {
                        sig,
                        static_: attrs.static_.is_some(),
                    };
                    public_methods.push(public_method);
                }
            }
        }

        public_methods
    }
    fn external_sig(&self) -> syn::Signature {
        let mut sig = self.sig.clone();
        if !self.static_ && sig.receiver().is_none() && !sig.inputs.is_empty() {
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
    pub(crate) fn definition(
        &self,
        ty: &TokenStream,
        sub_ty: &TokenStream,
        glib: &TokenStream,
    ) -> TokenStream {
        let proto = self.prototype();
        let ident = &self.sig.ident;
        let sig = self.external_sig();
        let args = signature_args(&sig);
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        if !self.static_ {
            if let Some(recv) = self.sig.receiver() {
                let has_ref = util::arg_reference(recv).is_some();
                let cast = match has_ref {
                    true => quote! { upcast_ref },
                    false => quote! { upcast },
                };
                let ref_ = (!has_ref).then(|| quote! { & });
                return quote! {
                    #proto {
                        #![inline]
                        let #this_ident = #glib::Cast::#cast::<#ty>(self);
                        let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ #this_ident);
                        #sub_ty::#ident(#this_ident, #(#args),*)
                    }
                };
            } else if let Some(first) = self.sig.inputs.first() {
                let cast = match util::arg_reference(first) {
                    Some(_) => quote! { upcast_ref },
                    None => quote! { upcast },
                };
                return quote! {
                    #proto {
                        #![inline]
                        let #this_ident = #glib::Cast::#cast::<#ty>(self);
                        #sub_ty::#ident(#this_ident, #(#args),*)
                    }
                };
            }
        }
        quote! {
            #proto {
                #![inline]
                #sub_ty::#ident(#(#args),*)
            }
        }
    }
}

#[inline]
fn signature_args(sig: &syn::Signature) -> impl Iterator<Item = &syn::Ident> {
    sig.inputs.iter().filter_map(|arg| {
        if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
            if let syn::Pat::Ident(syn::PatIdent { ident, .. }) = pat.as_ref() {
                return Some(ident);
            }
        }
        None
    })
}
