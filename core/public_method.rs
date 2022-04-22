use crate::{
    util::{self, Errors},
    TypeBase, TypeMode,
};
use darling::FromAttributes;
use heck::ToKebabCase;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;

#[derive(Debug, Clone)]
pub struct PublicMethod {
    pub sig: syn::Signature,
    pub target: Option<syn::Ident>,
    pub mode: TypeMode,
    pub constructor: Option<ConstructorType>,
    pub generic_args: util::GenericArgs,
    pub custom_body: Option<Box<syn::Expr>>,
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum ConstructorType {
    Auto(syn::Visibility, Box<syn::Signature>),
    Custom,
}

#[derive(Default, FromAttributes)]
#[darling(default, attributes(public, constructor))]
struct PublicMethodAttrs {
    name: Option<syn::Ident>,
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
                    if matches!(
                        &public_method.constructor,
                        Some(ConstructorType::Auto(_, _))
                    ) {
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
        let mut name = None;
        let mut constructor = None;
        if base == TypeBase::Class {
            if let Some(attrs) = util::extract_attrs(&mut method.attrs, "constructor") {
                let attrs = util::parse_attributes::<PublicMethodAttrs>(&attrs, errors);
                name = attrs.name;
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
                    constructor = Some(ConstructorType::Auto(
                        method.vis.clone(),
                        Box::new(method.sig.clone()),
                    ));
                } else {
                    constructor = Some(ConstructorType::Custom);
                }
            }
        }
        let mut public = false;
        if let Some(attrs) = util::extract_attrs(&mut method.attrs, "public") {
            let attrs = util::parse_attributes::<PublicMethodAttrs>(&attrs, errors);
            if let Some(n) = attrs.name {
                if name.is_some() {
                    errors.push_spanned(&n, "Duplicate `name` attribute");
                } else {
                    if n == method.sig.ident {
                        errors.push_spanned(
                            &n,
                            "`name` attribute cannot be the same as the function name",
                        );
                    }
                    name = Some(n);
                }
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
        let mut sig = method.sig.clone();
        let target = name.map(|n| std::mem::replace(&mut sig.ident, n));
        Some(Self {
            sig,
            target,
            mode,
            constructor,
            generic_args,
            custom_body: None,
        })
    }
    #[inline]
    pub fn is_static(&self) -> bool {
        self.constructor.is_some() || self.sig.receiver().is_none()
    }
    pub(crate) fn prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        if self.is_static() {
            return None;
        }
        let mut sig = util::external_sig(&self.sig);
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
        if select_auto {
            if let Some(ConstructorType::Auto(vis, orig_sig)) = self.constructor.as_ref() {
                let mut sig = util::external_sig(orig_sig);
                self.generic_args.substitute(&mut sig, glib);
                let cast_args = self.generic_args.cast_args(&sig, orig_sig, glib);
                let args = sig.inputs.iter().filter_map(|arg| {
                    let ident = util::arg_name(arg)?;
                    let span = match arg {
                        syn::FnArg::Receiver(r) => r.span(),
                        syn::FnArg::Typed(t) => t.ty.span(),
                    };
                    let name = ident.to_string().to_kebab_case();
                    Some(quote_spanned! { span => (#name, &#ident) })
                });
                return Some(quote_spanned! { orig_sig.span() =>
                    #vis #sig {
                        #![inline]
                        #cast_args
                        #glib::Object::new::<#wrapper_ty>(&[#(#args),*])
                            .expect("Failed to construct object")
                    }
                });
            } else {
                return None;
            }
        }
        if self.mode == TypeMode::Wrapper && self.target.is_none() && (final_ || select_statics) {
            return None;
        }
        let mut sig = util::external_sig(&self.sig);
        self.generic_args.substitute(&mut sig, glib);
        let cast_args = self.generic_args.cast_args(&sig, &self.sig, glib);
        if let Some(custom_body) = self.custom_body.as_ref() {
            return Some(quote_spanned! { self.sig.span() =>
                #sig {
                    #cast_args
                    #custom_body
                }
            });
        }
        let args = util::signature_args(&sig);
        let await_ = self.sig.asyncness.as_ref().map(|_| quote! { .await });
        let target = self.target.as_ref().unwrap_or(&sig.ident);
        let dest = match self.mode {
            TypeMode::Subclass => sub_ty,
            TypeMode::Wrapper => wrapper_ty,
        };
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
                #sig {
                    #![inline]
                    #cast_args
                    let #this_ident = #glib::Cast::#cast::<#wrapper_ty>(self);
                    #unwrap_recv
                    #dest::#target(#this_ident, #(#args),*) #await_
                }
            })
        } else {
            Some(quote_spanned! { self.sig.span() =>
                #sig {
                    #![inline]
                    #cast_args
                    #dest::#target(#(#args),*) #await_
                }
            })
        }
    }
}
