use crate::{util, TypeBase};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use std::collections::HashSet;

#[derive(Debug)]
pub struct VirtualMethod {
    method: syn::ImplItemMethod,
}

impl VirtualMethod {
    pub(crate) fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        errors: &mut Vec<darling::Error>,
    ) -> Vec<Self> {
        let mut virtual_method_names = HashSet::new();
        let mut virtual_methods = Vec::new();

        let mut index = 0;
        while index < items.len() {
            let mut method_attr = None;
            if let syn::ImplItem::Method(method) = &mut items[index] {
                let method_index = method
                    .attrs
                    .iter()
                    .position(|attr| attr.path.is_ident("virtual"));
                if let Some(method_index) = method_index {
                    method_attr.replace(method.attrs.remove(method_index));
                }
                if let Some(next) = method.attrs.first() {
                    util::push_error_spanned(errors, next, "Unknown attribute on virtual method");
                }
            }
            if let Some(attr) = method_attr {
                let method = match &items[index] {
                    syn::ImplItem::Method(method) => method.clone(),
                    _ => unreachable!(),
                };
                let virtual_method =
                    Self::from_method(method, attr, &mut virtual_method_names, errors);
                if let Some(virtual_method) = virtual_method {
                    virtual_methods.push(virtual_method);
                }
            } else {
                index += 1;
            }
        }

        virtual_methods
    }
    #[inline]
    fn from_method(
        method: syn::ImplItemMethod,
        attr: syn::Attribute,
        virtual_method_names: &mut HashSet<String>,
        errors: &mut Vec<darling::Error>,
    ) -> Option<Self> {
        if !attr.tokens.is_empty() {
            util::push_error_spanned(errors, &attr.tokens, "Unknown tokens on accumulator");
        }
        {
            let ident = &method.sig.ident;
            if virtual_method_names.contains(&ident.to_string()) {
                util::push_error_spanned(
                    errors,
                    ident,
                    format!("Duplicate definition for method `{}`", ident),
                );
                return None;
            }
        }
        if method
            .sig
            .receiver()
            .map(|r| match r {
                syn::FnArg::Receiver(syn::Receiver {
                    reference,
                    mutability,
                    ..
                }) => reference.is_none() || mutability.is_some(),
                _ => true,
            })
            .unwrap_or(true)
        {
            if let Some(first) = method.sig.inputs.first() {
                util::push_error_spanned(
                    errors,
                    first,
                    "First argument to method handler must be `&self`",
                );
            }
            return None;
        }
        Some(Self { method })
    }
    fn external_sig(&self) -> syn::Signature {
        // TODO - impl IsA args?
        let mut sig = self.method.sig.clone();
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
        let syn::ImplItemMethod {
            attrs,
            vis,
            defaultness,
            ..
        } = &self.method;
        let sig = self.external_sig();
        quote! {
            #(#attrs)* #vis #defaultness #sig
        }
    }
    pub(crate) fn definition(
        &self,
        ty: &TokenStream,
        base: TypeBase,
        glib: &TokenStream,
    ) -> TokenStream {
        let proto = self.prototype();
        let ident = &self.method.sig.ident;
        let external_sig = self.external_sig();
        let args = signature_args(&external_sig);
        let get_vtable = match base {
            TypeBase::Class => quote! {
                #glib::ObjectExt::class(____obj)
            },
            TypeBase::Interface => quote! {
                #glib::ObjectExt::interface::<#ty>(____obj).unwrap()
            },
        };
        quote_spanned! { Span::mixed_site() =>
            #proto {
                let ____obj = #glib::Cast::upcast_ref::<#ty>(self);
                let ____vtable = #get_vtable;
                let ____vtable = ::std::convert::AsRef::as_ref(____vtable);
                (____vtable.#ident)(____obj, #(#args),*)
            }
        }
    }
    fn parent_sig(&self, ident: syn::Ident, ty: &syn::Type) -> syn::Signature {
        let mut sig = self.external_sig();
        sig.ident = format_ident!("parent_{}", self.method.sig.ident);
        let this_index = sig.inputs.is_empty().then(|| 0).unwrap_or(1);
        sig.inputs.insert(
            this_index,
            syn::FnArg::Typed(syn::PatType {
                attrs: Vec::new(),
                pat: Box::new(syn::Pat::Ident(syn::PatIdent {
                    attrs: Default::default(),
                    by_ref: None,
                    mutability: None,
                    ident,
                    subpat: None,
                })),
                colon_token: Default::default(),
                ty: Box::new(ty.clone()),
            }),
        );
        sig
    }
    pub(crate) fn default_definition(&self, ty: &syn::Type, ext_trait: &syn::Ident) -> TokenStream {
        let syn::ImplItemMethod {
            attrs,
            vis,
            defaultness,
            ..
        } = &self.method;
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let mut sig = self.parent_sig(this_ident.clone(), ty);
        let parent_ident = std::mem::replace(&mut sig.ident, self.method.sig.ident.clone());
        let external_sig = self.external_sig();
        let args = signature_args(&external_sig);
        quote_spanned! { Span::mixed_site() =>
            #(#attrs)* #vis #defaultness #sig {
                #ext_trait::#parent_ident(self, #this_ident, #(#args),*)
            }
        }
    }
    pub(crate) fn parent_prototype(
        &self,
        ident: Option<syn::Ident>,
        ty: &syn::Type,
    ) -> TokenStream {
        let syn::ImplItemMethod {
            attrs,
            vis,
            defaultness,
            ..
        } = &self.method;
        let this_ident = ident.unwrap_or_else(|| syn::Ident::new("____this", Span::mixed_site()));
        let sig = self.parent_sig(this_ident, ty);
        quote! {
            #(#attrs)* #vis #defaultness #sig
        }
    }
    pub(crate) fn parent_definition(
        &self,
        mod_name: &syn::Ident,
        type_name: &syn::Ident,
        ty: &syn::Type,
        base: TypeBase,
        glib: &TokenStream,
    ) -> TokenStream {
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let proto = self.parent_prototype(Some(this_ident.clone()), ty);
        let ident = &self.method.sig.ident;
        let external_sig = self.external_sig();
        let args = signature_args(&external_sig);
        let class_name = format_ident!("{}Class", type_name);
        let vtable_ident = syn::Ident::new("____vtable", Span::mixed_site());
        let parent_vtable_method = match base {
            TypeBase::Class => quote! { parent_class },
            TypeBase::Interface => quote! { parent_interface::<#ty> },
        };
        quote_spanned! { Span::mixed_site() =>
            #proto {
                let #vtable_ident = <Self as #glib::subclass::types::ObjectSubclassType>::type_data();
                let #vtable_ident = &*(
                    #vtable_ident.as_ref().#parent_vtable_method()
                    as *mut #mod_name::#class_name
                );
                (#vtable_ident.#ident)(#this_ident, #(#args),*)
            }
        }
    }
    fn trampoline_sig(&self, ident: syn::Ident, ty: syn::Type) -> syn::Signature {
        let mut sig = self.external_sig();
        if let Some(syn::FnArg::Receiver(recv)) = sig.receiver().cloned() {
            sig.inputs[0] = syn::FnArg::Typed(syn::PatType {
                attrs: recv.attrs,
                pat: Box::new(syn::Pat::Ident(syn::PatIdent {
                    attrs: Vec::new(),
                    by_ref: None,
                    mutability: None,
                    ident,
                    subpat: None,
                })),
                colon_token: Default::default(),
                ty: Box::new(if let Some((and, lifetime)) = recv.reference {
                    syn::Type::Reference(syn::TypeReference {
                        and_token: and,
                        lifetime,
                        mutability: recv.mutability,
                        elem: Box::new(ty),
                    })
                } else {
                    ty
                }),
            });
        }
        sig
    }
    pub(crate) fn vtable_field(&self, wrapper_ty: &syn::Type) -> TokenStream {
        let ident = &self.method.sig.ident;
        let sig = self.trampoline_sig(ident.clone(), wrapper_ty.clone());
        let args = sig.inputs.iter().map(|arg| match arg {
            syn::FnArg::Typed(syn::PatType { ty, .. }) => ty.as_ref(),
            _ => unreachable!(),
        });
        quote! {
            #ident: fn(#(#args),*)
        }
    }
    #[inline]
    fn unwrap_recv(&self, ident: &syn::Ident, glib: &TokenStream) -> Option<TokenStream> {
        if self.method.sig.receiver().is_some() {
            Some(quote! {
                let #ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(&#ident);
            })
        } else {
            None
        }
    }
    pub(crate) fn set_default_trampoline(
        &self,
        type_name: &syn::Ident,
        ty: &syn::Type,
        class_ident: &syn::Ident,
        glib: &TokenStream,
    ) -> TokenStream {
        let ident = &self.method.sig.ident;
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let trampoline_ident = format_ident!("{}_default_trampoline", ident);
        let mut sig = self.trampoline_sig(this_ident.clone(), ty.clone());
        sig.ident = trampoline_ident.clone();
        let unwrap_recv = self.unwrap_recv(&this_ident, glib);
        let args = signature_args(&sig);
        quote_spanned! { Span::mixed_site() =>
            #sig {
                #unwrap_recv
                #type_name::#ident(#(#args),*)
            }
            #class_ident.#ident = #trampoline_ident;
        }
    }
    pub(crate) fn set_subclassed_trampoline(
        &self,
        ty: &syn::Type,
        trait_name: &syn::Ident,
        type_ident: &syn::Ident,
        class_ident: &syn::Ident,
        glib: &TokenStream,
    ) -> TokenStream {
        let ident = &self.method.sig.ident;
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let trampoline_ident = format_ident!("{}_trampoline", ident);
        let mut sig = self.trampoline_sig(this_ident.clone(), ty.clone());
        sig.ident = trampoline_ident.clone();
        let param = util::parse(
            quote! { #type_ident: #glib::subclass::types::ObjectSubclass + #trait_name },
            &mut vec![],
        )
        .unwrap();
        sig.generics.params.push(param);
        let unwrap_recv = self.unwrap_recv(&this_ident, glib);
        let args = signature_args(&sig);
        quote_spanned! { Span::mixed_site() =>
            #sig {
                let #this_ident = #glib::Cast::dynamic_cast_ref::<<#type_ident as #glib::subclass::types::ObjectSubclass>::Type>(
                    &#this_ident
                ).unwrap();
                #unwrap_recv
                #trait_name::#ident(#this_ident, #(#args),*)
            }
            #class_ident.#ident = #trampoline_ident::<#type_ident>;
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
