use std::collections::HashSet;

use crate::{
    util::{self, Errors},
    TypeBase,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use syn::parse_quote;

#[derive(Debug)]
pub struct VirtualMethod {
    pub attrs: Vec<syn::Attribute>,
    pub vis: syn::Visibility,
    pub sig: syn::Signature,
    pub generic_args: HashSet<usize>,
    pub base: TypeBase,
}

impl VirtualMethod {
    pub(crate) fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        base: TypeBase,
        errors: &Errors,
    ) -> Vec<Self> {
        let mut virtual_methods = Vec::new();

        for item in items {
            if let syn::ImplItem::Method(method) = item {
                let method_index = method
                    .attrs
                    .iter()
                    .position(|attr| attr.path.is_ident("virt"));
                if let Some(method_index) = method_index {
                    let attr = method.attrs.remove(method_index);
                    let virtual_method = Self::from_method(method, attr, base, errors);
                    if let Some(virtual_method) = virtual_method {
                        virtual_methods.push(virtual_method);
                    }
                }
            }
        }

        virtual_methods
    }
    #[inline]
    fn from_method(
        method: &mut syn::ImplItemMethod,
        attr: syn::Attribute,
        base: TypeBase,
        errors: &Errors,
    ) -> Option<Self> {
        if !attr.tokens.is_empty() {
            errors.push_spanned(&attr.tokens, "Unknown tokens on virtual method");
        }
        let syn::ImplItemMethod {
            attrs, vis, sig, ..
        } = method;
        if sig.inputs.is_empty() {
            errors.push_spanned(
                &sig.inputs,
                "First argument required on virtual method, must be `self`, `&self` or the wrapper type",
            );
        }
        if base == TypeBase::Interface {
            if let Some(recv) = sig.receiver() {
                errors.push_spanned(
                    recv,
                    "First argument to interface virtual method must be the wrapper type",
                );
            }
        }
        let generic_args = sig
            .inputs
            .iter_mut()
            .enumerate()
            .filter_map(|(i, arg)| {
                if let syn::FnArg::Typed(t) = arg {
                    let index = t.attrs.iter().position(|a| a.path.is_ident("is_a"));
                    if let Some(index) = index {
                        t.attrs.remove(index);
                        return Some(i);
                    }
                }
                None
            })
            .collect();
        Some(Self {
            attrs: attrs.clone(),
            vis: vis.clone(),
            sig: sig.clone(),
            generic_args,
            base,
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
        if sig.receiver().is_none() {
            if let Some(arg) = sig.inputs.first_mut() {
                let ref_ = util::arg_reference(arg);
                *arg = parse_quote! { #ref_ self };
            }
        }
        sig
    }
    fn public_sig(&self, glib: &TokenStream) -> syn::Signature {
        let mut sig = self.external_sig();
        for (index, arg) in sig.inputs.iter_mut().enumerate() {
            if self.generic_args.contains(&index) {
                let ref_ = util::arg_reference(arg);
                if let syn::FnArg::Typed(t) = arg {
                    let ty = &*t.ty;
                    let (ref_, ty) = match ty {
                        syn::Type::Reference(r) => (ref_, &*r.elem),
                        ty => (None, ty),
                    };
                    t.ty = Box::new(parse_quote! { #ref_ impl #glib::IsA<#ty> });
                } else {
                    unreachable!()
                }
            }
        }
        sig
    }
    pub(crate) fn prototype(&self, glib: &TokenStream) -> TokenStream {
        let sig = self.public_sig(glib);
        quote! { #sig }
    }
    pub(crate) fn definition(&self, wrapper_ty: &TokenStream, glib: &TokenStream) -> TokenStream {
        let ident = &self.sig.ident;
        let sig = self.public_sig(glib);
        let args = signature_args(&sig);
        let obj_ident = syn::Ident::new("____obj", Span::mixed_site());
        let vtable_ident = syn::Ident::new("____vtable", Span::mixed_site());
        let get_vtable = match self.base {
            TypeBase::Class => quote! {
                #glib::ObjectExt::class(#obj_ident)
            },
            TypeBase::Interface => quote! {
                #glib::ObjectExt::interface::<#wrapper_ty>(#obj_ident).unwrap()
            },
        };
        let deref_vtable = match self.base {
            TypeBase::Class => quote! {
                ::std::convert::AsRef::as_ref(#vtable_ident)
            },
            TypeBase::Interface => quote! {
                ::std::convert::AsRef::as_ref(&*#vtable_ident)
            },
        };
        let cast_args = sig
            .inputs
            .iter()
            .enumerate()
            .skip(1)
            .filter_map(|(i, arg)| {
                let orig = match &self.sig.inputs[i] {
                    syn::FnArg::Typed(t) => t,
                    _ => return None,
                };
                self.generic_args
                    .contains(&i)
                    .then(|| arg_name(arg))
                    .flatten()
                    .map(|name| {
                        let (cast, ty) = match &*orig.ty {
                            syn::Type::Reference(r) => (quote! { upcast_ref }, &*r.elem),
                            ty => (quote! { upcast }, ty),
                        };
                        quote! {
                            let #name = #glib::Cast::#cast::<#ty>(#name);
                        }
                    })
            });
        quote! {
            #sig {
                #![inline]
                let #obj_ident = #glib::Cast::upcast_ref::<#wrapper_ty>(self);
                let #vtable_ident = #get_vtable;
                let #vtable_ident = #deref_vtable;
                #(#cast_args)*
                (#vtable_ident.#ident)(#obj_ident, #(#args),*)
            }
        }
    }
    fn parent_sig(&self, ident: &syn::Ident, glib: &TokenStream) -> syn::Signature {
        let mut sig = self.external_sig();
        sig.ident = format_ident!("parent_{}", self.sig.ident);
        if !sig.inputs.is_empty() {
            sig.inputs.insert(
                1,
                parse_quote! {
                    #ident: &<Self as #glib::subclass::types::ObjectSubclass>::Type
                },
            );
        }
        sig
    }
    pub(crate) fn default_definition(
        &self,
        ext_trait: &syn::Ident,
        glib: &TokenStream,
    ) -> TokenStream {
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let mut sig = self.parent_sig(&this_ident, glib);
        let parent_ident = std::mem::replace(&mut sig.ident, self.sig.ident.clone());
        let external_sig = self.external_sig();
        let args = signature_args(&external_sig);
        quote! {
            #sig {
                #![inline]
                #ext_trait::#parent_ident(self, #this_ident, #(#args),*)
            }
        }
    }
    pub(crate) fn parent_prototype(&self, glib: &TokenStream) -> TokenStream {
        let mut name = String::from("obj");
        while signature_args(&self.sig).any(|i| *i == name) {
            name.insert(0, '_');
        }
        let this_ident = syn::Ident::new(&name, Span::mixed_site());
        let sig = self.parent_sig(&this_ident, glib);
        quote! { #sig }
    }
    pub(crate) fn parent_definition(&self, ty: &syn::Type, glib: &TokenStream) -> TokenStream {
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let sig = self.parent_sig(&this_ident, glib);
        let ident = &self.sig.ident;
        let args = signature_args(&sig);
        let vtable_ident = syn::Ident::new("____vtable", Span::mixed_site());
        let parent_vtable_method = match self.base {
            TypeBase::Class => quote! { parent_class },
            TypeBase::Interface => quote! { parent_interface::<#ty> },
        };
        quote! {
            #sig {
                #![inline]
                let #this_ident = unsafe {
                    #glib::Cast::unsafe_cast_ref::<#ty>(#this_ident)
                };
                let #vtable_ident = <Self as #glib::subclass::types::ObjectSubclassType>::type_data();
                let #vtable_ident = unsafe {
                    &*(
                        #vtable_ident.as_ref().#parent_vtable_method()
                        as *mut <#ty as #glib::object::ObjectType>::GlibClassType
                    )
                };
                (#vtable_ident.#ident)(#(#args),*)
            }
        }
    }
    fn trampoline_sig(&self, ident: syn::Ident, ty: syn::Type) -> syn::Signature {
        let mut sig = self.external_sig();
        match sig.receiver().cloned() {
            Some(ref arg @ syn::FnArg::Receiver(ref recv)) => {
                let attrs = &recv.attrs;
                let ref_ = util::arg_reference(arg);
                sig.inputs[0] = parse_quote! {
                    #(#attrs)* #ident: #ref_ #ty
                };
            }
            Some(syn::FnArg::Typed(mut pat)) => {
                pat.pat = parse_quote! { #ident };
                if let syn::Type::Reference(r) = &mut *pat.ty {
                    r.elem = Box::new(ty);
                } else {
                    pat.ty = Box::new(ty);
                }
                sig.inputs[0] = syn::FnArg::Typed(pat);
            }
            _ => {}
        }
        sig
    }
    pub(crate) fn vtable_field(&self, wrapper_ty: &syn::Type) -> TokenStream {
        let ident = &self.sig.ident;
        let sig = self.trampoline_sig(ident.clone(), wrapper_ty.clone());
        let output = &sig.output;
        let args = sig.inputs.iter().map(|arg| match arg {
            syn::FnArg::Typed(syn::PatType { ty, .. }) => ty.as_ref(),
            _ => unreachable!(),
        });
        quote! {
            #ident: fn(#(#args),*) #output
        }
    }
    pub(crate) fn set_default_trampoline(
        &self,
        type_name: &syn::Ident,
        ty: &syn::Type,
        class_ident: &TokenStream,
        glib: &TokenStream,
    ) -> TokenStream {
        let ident = &self.sig.ident;
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let trampoline_ident = format_ident!("{}_default_trampoline", ident);
        let mut sig = self.trampoline_sig(this_ident.clone(), ty.clone());
        sig.ident = trampoline_ident.clone();
        let unwrap_recv = self.sig.receiver().map(|_| {
            quote! {
                let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#this_ident);
            }
        });
        let args = signature_args(&sig);
        quote! {
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
        let ident = &self.sig.ident;
        let this_ident = syn::Ident::new("____this", Span::mixed_site());
        let imp_ident = syn::Ident::new("____imp", Span::mixed_site());
        let trampoline_ident = format_ident!("{}_trampoline", ident);
        let mut sig = self.trampoline_sig(this_ident.clone(), ty.clone());
        sig.ident = trampoline_ident.clone();
        let param = syn::parse_quote! {
            #type_ident: #glib::subclass::types::ObjectSubclass + #trait_name
        };
        sig.generics.params.push(param);
        let args = signature_args(&sig);
        quote! {
            #sig {
                let #this_ident = #glib::Cast::dynamic_cast_ref::<<#type_ident as #glib::subclass::types::ObjectSubclass>::Type>(
                    #this_ident
                ).unwrap();
                let #imp_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#this_ident);
                #trait_name::#ident(#imp_ident, #(#args),*)
            }
            #class_ident.#ident = #trampoline_ident::<#type_ident>;
        }
    }
}

#[inline]
fn signature_args(sig: &syn::Signature) -> impl Iterator<Item = &syn::Ident> + Clone {
    sig.inputs.iter().filter_map(arg_name)
}

#[inline]
fn arg_name(arg: &syn::FnArg) -> Option<&syn::Ident> {
    if let syn::FnArg::Typed(syn::PatType { pat, .. }) = arg {
        if let syn::Pat::Ident(syn::PatIdent { ident, .. }) = pat.as_ref() {
            return Some(ident);
        }
    }
    None
}
