use crate::{
    util::{self, Errors},
    TypeBase, TypeMode,
};
use heck::ToKebabCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse_quote;

#[derive(Debug)]
pub struct PublicMethod {
    pub sig: syn::Signature,
    pub mode: TypeMode,
    pub constructor: Option<ConstructorType>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum ConstructorType {
    Auto,
    Custom,
}

impl PublicMethod {
    pub(crate) fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        base: TypeBase,
        mode: TypeMode,
        errors: &Errors,
    ) -> Vec<Self> {
        let mut public_methods = Vec::new();

        let mut to_remove = Vec::new();
        for (index, item) in items.iter_mut().enumerate() {
            if let syn::ImplItem::Method(method) = item {
                let public_method = Self::from_method(method, base, mode, errors);
                if let Some(public_method) = public_method {
                    if public_method.constructor == Some(ConstructorType::Auto) {
                        to_remove.push(index);
                    }
                    public_methods.push(public_method);
                }
            }
        }

        for index in to_remove.into_iter().rev() {
            items.remove(index);
        }

        public_methods
    }
    #[inline]
    fn from_method(
        method: &mut syn::ImplItemMethod,
        base: TypeBase,
        mode: TypeMode,
        errors: &Errors,
    ) -> Option<Self> {
        let mut constructor = None;
        if base == TypeBase::Class {
            if let Some(attr) = util::extract_attr(&mut method.attrs, "constructor") {
                if !attr.tokens.is_empty() {
                    errors.push_spanned(&attr.tokens, "Unknown tokens on `constructor` attribute");
                }
                if let Some(recv) = method.sig.receiver() {
                    errors.push_spanned(recv, "`self` not allowed on constructor");
                }
                if matches!(&method.sig.output, syn::ReturnType::Default) {
                    errors.push_spanned(&method.sig, "Constructor must have a return type");
                }
                if method.block.stmts.is_empty() {
                    for arg in &method.sig.inputs {
                        if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
                            match pat.as_ref() {
                                syn::Pat::Ident(_) => {}
                                p => errors
                                    .push_spanned(p, "Auto constructor argument must be an ident"),
                            }
                        }
                    }
                    constructor = Some(ConstructorType::Auto);
                } else {
                    constructor = Some(ConstructorType::Custom);
                }
            }
        }
        let mut public = false;
        if let Some(attr) = util::extract_attr(&mut method.attrs, "public") {
            if !attr.tokens.is_empty() {
                errors.push_spanned(&attr.tokens, "Unknown tokens on `public` attribute");
            }
            public = true;
        }
        if !public && constructor.is_none() {
            return None;
        }
        if mode == TypeMode::Subclass && base == TypeBase::Interface {
            if let Some(recv) = method.sig.receiver() {
                errors.push_spanned(
                    recv,
                    r"`self` not supported on public method for private interface struct. \
                    Implement this method on the wrapper type",
                );
            }
        }
        let sig = method.sig.clone();
        Some(Self {
            sig,
            mode,
            constructor,
        })
    }
    fn external_sig(&self) -> syn::Signature {
        let mut sig = self.sig.clone();
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
    #[inline]
    pub fn is_static(&self) -> bool {
        self.constructor.is_some() || self.sig.receiver().is_none()
    }
    pub(crate) fn prototype(&self) -> Option<TokenStream> {
        if self.is_static() {
            return None;
        }
        let sig = self.external_sig();
        Some(quote! { #sig })
    }
    pub(crate) fn definition(
        &self,
        wrapper_ty: &TokenStream,
        sub_ty: &TokenStream,
        select_statics: bool,
        final_: bool,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        if select_statics != self.is_static() {
            return None;
        }
        if self.mode == TypeMode::Wrapper
            && (final_ || self.constructor == Some(ConstructorType::Custom))
        {
            return None;
        }
        let proto = self.external_sig();
        let ident = &self.sig.ident;
        let sig = self.external_sig();
        let args = util::signature_args(&sig);
        let dest = match self.mode {
            TypeMode::Subclass => sub_ty,
            TypeMode::Wrapper => wrapper_ty,
        };
        if self.constructor == Some(ConstructorType::Auto) {
            let args = args.map(|ident| {
                let name = ident.to_string().to_kebab_case();
                quote! { (#name, &#ident) }
            });
            return Some(quote! {
                #proto {
                    #![inline]
                    #glib::Object::new::<#wrapper_ty>(&[#(#args),*])
                        .expect("Failed to construct object")
                }
            });
        }
        if let Some(recv) = self.sig.receiver() {
            let has_ref = util::arg_reference(recv).is_some();
            let this_ident = util::arg_name(recv)
                .cloned()
                .unwrap_or_else(|| syn::Ident::new("____this", Span::mixed_site()));
            let cast = match has_ref {
                true => quote! { upcast_ref },
                false => quote! { upcast },
            };
            let unwrap_recv = (self.mode == TypeMode::Subclass).then(|| {
                let ref_ = (!has_ref).then(|| quote! { & });
                quote! {
                    let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ #this_ident);
                }
            });
            Some(quote! {
                #proto {
                    #![inline]
                    let #this_ident = #glib::Cast::#cast::<#wrapper_ty>(self);
                    #unwrap_recv
                    #dest::#ident(#this_ident, #(#args),*)
                }
            })
        } else {
            Some(quote! {
                #proto {
                    #![inline]
                    #dest::#ident(#(#args),*)
                }
            })
        }
    }
}
