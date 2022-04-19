use crate::{
    util::{self, Errors},
    TypeBase, TypeMode,
};
use heck::ToKebabCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::{parse_quote, spanned::Spanned};

#[derive(Debug)]
pub struct PublicMethod {
    pub sig: syn::Signature,
    pub mode: TypeMode,
    pub constructor: Option<ConstructorType>,
    pub generic_args: util::GenericArgs,
    pub custom_body: Option<Box<syn::Expr>>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum ConstructorType {
    Auto(syn::Visibility),
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
                    if matches!(public_method.constructor, Some(ConstructorType::Auto(_))) {
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
        if base == TypeBase::Class && mode == TypeMode::Wrapper {
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
                    constructor = Some(ConstructorType::Auto(method.vis.clone()));
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
        let generic_args = match mode {
            TypeMode::Subclass => util::GenericArgs::new(&mut method.sig),
            TypeMode::Wrapper => Default::default(),
        };
        let sig = method.sig.clone();
        Some(Self {
            sig,
            mode,
            constructor,
            generic_args,
            custom_body: None,
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
    pub(crate) fn prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        if self.is_static() {
            return None;
        }
        let mut sig = self.external_sig();
        self.generic_args.substitute(&mut sig, glib);
        Some(quote! { #sig })
    }
    pub(crate) fn definition(
        &self,
        wrapper_ty: &TokenStream,
        sub_ty: &TokenStream,
        select_statics: bool,
        select_auto: bool,
        final_: bool,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        if select_statics != self.is_static() {
            return None;
        }
        let is_auto = matches!(self.constructor, Some(ConstructorType::Auto(_)));
        if select_auto != is_auto {
            return None;
        }
        if self.mode == TypeMode::Wrapper && !is_auto && (final_ || select_statics) {
            return None;
        }
        let mut sig = self.external_sig();
        self.generic_args.substitute(&mut sig, glib);
        let proto = match &self.constructor {
            Some(ConstructorType::Auto(vis)) => quote! { #vis #sig },
            _ => quote! { #sig },
        };
        if let Some(custom_body) = self.custom_body.as_ref() {
            return Some(quote_spanned! { self.sig.span() =>
                #proto {
                    #custom_body
                }
            });
        }
        let ident = &sig.ident;
        let dest = match self.mode {
            TypeMode::Subclass => sub_ty,
            TypeMode::Wrapper => wrapper_ty,
        };
        if is_auto {
            let args = sig.inputs.iter().filter_map(|arg| {
                let ident = util::arg_name(arg)?;
                let span = match arg {
                    syn::FnArg::Receiver(r) => r.span(),
                    syn::FnArg::Typed(t) => t.ty.span(),
                };
                let name = ident.to_string().to_kebab_case();
                Some(quote_spanned! { span => (#name, &#ident) })
            });
            return Some(quote_spanned! { self.sig.span() =>
                #proto {
                    #![inline]
                    #glib::Object::new::<#wrapper_ty>(&[#(#args),*])
                        .expect("Failed to construct object")
                }
            });
        }
        let args = util::signature_args(&sig);
        let cast_args = self.generic_args.cast_args(&sig, &self.sig, glib);
        let await_ = self.sig.asyncness.as_ref().map(|_| quote! { .await });
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
                quote_spanned! { recv.span() =>
                    let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ #this_ident);
                }
            });
            Some(quote_spanned! { self.sig.span() =>
                #proto {
                    #![inline]
                    #cast_args
                    let #this_ident = #glib::Cast::#cast::<#wrapper_ty>(self);
                    #unwrap_recv
                    #dest::#ident(#this_ident, #(#args),*) #await_
                }
            })
        } else {
            Some(quote_spanned! { self.sig.span() =>
                #proto {
                    #![inline]
                    #cast_args
                    #dest::#ident(#(#args),*) #await_
                }
            })
        }
    }
}
