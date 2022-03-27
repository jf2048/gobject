use crate::util;
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashSet;

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
                    util::push_error_spanned(
                        errors,
                        next,
                        "Unknown attribute on virtual method"
                    );
                }
            }
            if let Some(attr) = method_attr {
                let sub = items.remove(index);
                let method = match sub {
                    syn::ImplItem::Method(method) => method,
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
    fn from_method<'methods>(
        method: syn::ImplItemMethod,
        attr: syn::Attribute,
        virtual_method_names: &mut HashSet<String>,
        errors: &mut Vec<darling::Error>,
    ) -> Option<Self> {
        if !attr.tokens.is_empty() {
            util::push_error_spanned(
                errors,
                &attr.tokens,
                "Unknown tokens on accumulator"
            );
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
        if method.sig.receiver().map(|r| match r {
            syn::FnArg::Receiver(syn::Receiver { reference, mutability, .. }) => {
                reference.is_none() || mutability.is_some()
            },
            _ => true,
        }).unwrap_or(true) {
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
        for (index, arg) in sig.inputs.iter_mut().skip(1).enumerate() {
            if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
                if !matches!(**pat, syn::Pat::Ident(_)) {
                    *pat = Box::new(syn::Pat::Ident(syn::PatIdent {
                        attrs: Default::default(),
                        by_ref: None,
                        mutability: None,
                        ident: format_ident!("arg{}", index),
                        subpat: None
                    }));
                }
            }
        }
        sig
    }
    fn external_args(&self) -> Vec<syn::Ident> {
        let mut args = vec![];
        for arg in self.external_sig().inputs {
            if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
                if let syn::Pat::Ident(syn::PatIdent { ident, .. }) = *pat {
                    args.push(ident);
                }
            }
        }
        args
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
    pub(crate) fn definition(&self, ty: &syn::Type, is_interface: bool, glib: &TokenStream) -> TokenStream {
        let proto = self.prototype();
        let ident = &self.method.sig.ident;
        let args = self.external_args();
        let get_vtable = if is_interface {
            quote! { #glib::ObjectExt::interface::<#ty>(____obj).unwrap() }
        } else {
            quote! { #glib::ObjectExt::class(____obj) }
        };
        quote! {
            #proto {
                let ____obj = #glib::Cast::upcast_ref::<#ty>(self);
                let ____vtable = #get_vtable;
                let ____vtable = ::std::convert::AsRef::as_ref(____vtable);
                (____vtable.#ident)(____obj, #(#args),*)
            }
        }
    }
    fn parent_sig(&self, ty: &syn::Type) -> syn::Signature {
        let mut sig = self.external_sig();
        sig.ident = format_ident!("parent_{}", self.method.sig.ident);
        let this_index = sig.inputs.is_empty().then(|| 0).unwrap_or(1);
        sig.inputs.insert(this_index, syn::FnArg::Typed(syn::PatType {
            attrs: Vec::new(),
            pat: Box::new(syn::Pat::Ident(syn::PatIdent {
                attrs: Default::default(),
                by_ref: None,
                mutability: None,
                ident: format_ident!("____this"),
                subpat: None
            })),
            colon_token: Default::default(),
            ty: Box::new(ty.clone()),
        }));
        sig
    }
    pub(crate) fn default_definition(&self, ty: &syn::Type) -> TokenStream {
        let syn::ImplItemMethod {
            attrs,
            vis,
            defaultness,
            ..
        } = &self.method;
        let mut sig = self.parent_sig(ty);
        let parent_ident = std::mem::replace(&mut sig.ident, self.method.sig.ident.clone());
        let args = self.external_args();
        quote! {
            #(#attrs)* #vis #defaultness #sig {
                self.#parent_ident(____this, #(#args),*)
            }
        }
    }
    pub(crate) fn parent_prototype(&self, ty: &syn::Type) -> TokenStream {
        let syn::ImplItemMethod {
            attrs,
            vis,
            defaultness,
            ..
        } = &self.method;
        let sig = self.parent_sig(ty);
        quote! {
            #(#attrs)* #vis #defaultness #sig
        }
    }
    pub(crate) fn parent_definition(&self, mod_name: &syn::Ident, name: &syn::Ident, ty: &syn::Type) -> TokenStream {
        let proto = self.parent_prototype(ty);
        let ident = &self.method.sig.ident;
        let args = self.external_args();
        let class_name = format_ident!("{}Class", name);
        todo!("support interfaces, fully qualify this stuff");
        quote! {
            #proto {
                let ____data = Self::type_data();
                let ____parent_class = &*(data.as_ref().parent_class() as *mut #mod_name::#class_name);
                (____parent_class.#ident)(____this, #(#args),*)
            }
        }
    }
    pub(crate) fn set_default_trampoline(&self) -> TokenStream {
        let ident = &self.method.sig.ident;
        let trampoline_ident = format_ident!("{}_default_trampoline", ident);
        quote! {
            fn #trampoline_ident() {}
            klass.#ident = #trampoline_ident;
        }
    }
    pub(crate) fn set_subclassed_trampoline(&self) -> TokenStream {
        let ident = &self.method.sig.ident;
        let trampoline_ident = format_ident!("{}_trampoline", ident);
        quote! {
            fn #trampoline_ident() {}
            klass.#ident = #trampoline_ident;
        }
    }
}
