use gobject_core::{
    util::{self, Errors},
    ConstructorType, TypeContext, TypeMode,
};
use proc_macro2::Span;
use quote::{quote, quote_spanned};
use syn::{parse_quote, parse_quote_spanned, spanned::Spanned};

pub(crate) fn extend_initables(def: &mut gobject_core::ClassDefinition, errors: &Errors) {
    extend_initable(def, errors);
    extend_async_initable(def, errors);
}

#[inline]
fn extend_initable(def: &mut gobject_core::ClassDefinition, errors: &Errors) {
    let (arg_count, span) = match def.inner.find_method(TypeMode::Subclass, "init") {
        Some(method) => (method.sig.inputs.len(), method.span()),
        None => return,
    };
    let name = match &def.inner.name {
        Some(name) => name,
        None => return,
    };
    let (sub_ty, wrapper_ty) = {
        use TypeContext::*;
        use TypeMode::*;
        match def
            .inner
            .type_(Subclass, Subclass, External)
            .zip(def.inner.type_(Subclass, Wrapper, External))
        {
            Some(tys) => tys,
            _ => return,
        }
    };
    let go = &def.inner.crate_ident;
    def.implements
        .push(syn::parse_quote! { #go::gio::Initable });
    let head = def.inner.trait_head(
        &parse_quote! { #name },
        quote! { #go::gio::subclass::prelude::InitableImpl },
    );
    let self_ident = syn::Ident::new("self", Span::mixed_site());
    let this_ident = syn::Ident::new("_obj", Span::mixed_site());
    let cancellable_ident = syn::Ident::new("____cancellable", Span::mixed_site());
    let has_cancellable = arg_count > 2;
    let args = [
        Some(&self_ident),
        (arg_count > 1).then(|| &this_ident),
        has_cancellable.then(|| &cancellable_ident),
    ]
    .into_iter()
    .flatten();
    def.inner
        .module
        .content
        .get_or_insert_with(Default::default)
        .1
        .push(syn::Item::Verbatim(quote_spanned! { span =>
            #head {
                fn init(
                    &#self_ident,
                    #this_ident: &<Self as #go::glib::subclass::types::ObjectSubclass>::Type,
                    #cancellable_ident: ::std::option::Option<&#go::gio::Cancellable>
                ) -> ::std::result::Result<(), #go::glib::Error> {
                    #name::init(#(#args),*)
                }
            }
        }));

    let glib = quote! { #go::glib };
    for pm in &mut def.inner.public_methods {
        if let Some(constructor) = pm.constructor.as_ref() {
            if let Some((custom_tag, _)) = pm.custom_body.as_ref() {
                errors.push(
                    pm.sig.span(),
                    format!("Initable constructor already overriden by {}", custom_tag),
                );
            }
            if pm.mode == TypeMode::Wrapper && pm.target.is_none() {
                errors.push(
                    pm.sig.span(),
                    "custom constructor on wrapper type for Initable must be renamed with #[public(name = \"...\")]",
                );
            }
            let target = pm.target.as_ref().unwrap_or(&pm.sig.ident);
            let orig_sig = match constructor {
                ConstructorType::Custom { .. } => &pm.sig,
                ConstructorType::Auto { sig, .. } => sig,
            };
            let mut sig = util::external_sig(orig_sig);
            pm.generic_args.substitute(&mut sig, &glib);
            let args = util::signature_args(&sig);
            let cancellable = if has_cancellable {
                quote! { #cancellable_ident }
            } else {
                quote! { #go::gio::Cancellable::NONE }
            };
            let fallible = constructor.fallible();
            if fallible {
                pm.sig.output = parse_quote! {
                    -> ::std::result::Result<Self, #go::gio::prelude::InitableError>
                };
            }
            let dest = match pm.mode {
                TypeMode::Subclass => &sub_ty,
                TypeMode::Wrapper => &wrapper_ty,
            };
            let construct_try = fallible.then(|| quote! { ? });
            let init_try = fallible.then(|| quote! { ? }).unwrap_or_else(|| {
                quote_spanned! { span =>
                    .unwrap_or_else(|e| {
                        ::std::panic!(
                            "Failed to construct {}: {:?}",
                            <#wrapper_ty as #glib::StaticType>::static_type().name(),
                            e,
                        );
                    })
                }
            });
            let ret = fallible
                .then(|| quote! { ::std::result::Result::Ok(#this_ident) })
                .unwrap_or_else(|| quote! { #this_ident });
            pm.custom_body = Some((
                String::from("Initable"),
                Box::new(parse_quote_spanned! { span => {
                    let #this_ident = #dest::#target(#(#args),*) #construct_try;
                    unsafe {
                        #go::gio::prelude::InitableExt::init(
                            &#this_ident,
                            #cancellable
                        )
                    } #init_try;
                    #ret
                } }),
            ));
            if has_cancellable {
                pm.sig.inputs.push(parse_quote! {
                    #cancellable_ident: ::std::option::Option<&impl #go::glib::isA<#go::gio::Cancellable>>
                });
            }
        }
    }
}

#[inline]
fn extend_async_initable(def: &mut gobject_core::ClassDefinition, errors: &Errors) {
    let (arg_count, span) = match def.inner.find_method(TypeMode::Subclass, "init_future") {
        Some(method) => (method.sig.inputs.len(), method.span()),
        None => return,
    };
    let name = match &def.inner.name {
        Some(name) => name,
        None => return,
    };
    let (sub_ty, wrapper_ty) = {
        use TypeContext::*;
        use TypeMode::*;
        match def
            .inner
            .type_(Subclass, Subclass, External)
            .zip(def.inner.type_(Subclass, Wrapper, External))
        {
            Some(tys) => tys,
            _ => return,
        }
    };
    let go = &def.inner.crate_ident;
    def.implements
        .push(syn::parse_quote! { #go::gio::AsyncInitable });
    let head = def.inner.trait_head(
        &parse_quote! { #name },
        quote! { #go::gio::subclass::prelude::AsyncInitableImpl },
    );
    let self_ident = syn::Ident::new("self", Span::mixed_site());
    let this_ident = syn::Ident::new("_obj", Span::mixed_site());
    let priority_ident = syn::Ident::new("____priority", Span::mixed_site());
    let has_priority = arg_count > 2;
    let args = [
        Some(&self_ident),
        (arg_count > 1).then(|| &this_ident),
        has_priority.then(|| &priority_ident),
    ]
    .into_iter()
    .flatten();
    def.inner
        .module
        .content
        .get_or_insert_with(Default::default)
        .1
        .push(syn::Item::Verbatim(quote_spanned! { span =>
            #head {
                fn init_future(
                    &#self_ident,
                    #this_ident: &<Self as #go::glib::subclass::types::ObjectSubclass>::Type,
                    #priority_ident: #go::glib::Priority,
                ) -> ::std::pin::Pin<::std::boxed::Box<dyn ::std::future::Future<Output = ::std::result::Result<(), #go::glib::Error>> + 'static>> {
                    #name::init_future(#(#args),*)
                }
            }
        }));

    let init_arg_count = def
        .inner
        .find_method(TypeMode::Subclass, "init")
        .map(|m| m.sig.inputs.len());
    let glib = quote! { #go::glib };
    let mut new_async_constructors = Vec::new();
    for pm in &mut def.inner.public_methods {
        if let Some(constructor) = pm.constructor.clone() {
            if let Some((custom_tag, _)) = pm.custom_body.as_ref() {
                if custom_tag != "Initable" {
                    errors.push(
                        pm.sig.span(),
                        format!(
                            "AsyncInitable constructor already overriden by {}",
                            custom_tag
                        ),
                    );
                }
            }
            let pm = if let Some(init_arg_count) = init_arg_count {
                let mut pm = pm.clone();
                pm.sig.ident = quote::format_ident!("{}_future", pm.sig.ident);
                if init_arg_count > 2 {
                    // remove the cancellable argument
                    pm.sig.inputs.pop();
                }
                new_async_constructors.push(pm);
                new_async_constructors.last_mut().unwrap()
            } else {
                if pm.mode == TypeMode::Wrapper && pm.target.is_none() {
                    errors.push(
                        pm.sig.span(),
                        "custom constructor on wrapper type for AsyncInitable must be renamed with #[public(name = \"...\")]",
                    );
                }
                pm
            };
            let target = pm.target.as_ref().unwrap_or(&pm.sig.ident);
            let orig_sig = match &constructor {
                ConstructorType::Custom { .. } => &pm.sig,
                ConstructorType::Auto { sig, .. } => sig,
            };
            let mut sig = util::external_sig(orig_sig);
            pm.generic_args.substitute(&mut sig, &glib);
            let args = util::signature_args(&sig);
            let priority = if has_priority {
                quote! { #priority_ident }
            } else {
                quote! { #go::glib::PRIORITY_DEFAULT }
            };
            pm.sig.asyncness = Some(Default::default());
            let fallible = constructor.fallible();
            if fallible {
                pm.sig.output = parse_quote! {
                    -> ::std::result::Result<Self, #go::gio::prelude::InitableError>
                };
            }
            let dest = match pm.mode {
                TypeMode::Subclass => &sub_ty,
                TypeMode::Wrapper => &wrapper_ty,
            };

            let construct_try = fallible.then(|| quote! { ? });
            let init_try = fallible.then(|| quote! { ? }).unwrap_or_else(|| {
                quote_spanned! { span =>
                    .unwrap_or_else(|e| {
                        ::std::panic!(
                            "Failed to construct {}: {:?}",
                            <#wrapper_ty as #glib::StaticType>::static_type().name(),
                            e,
                        );
                    })
                }
            });
            let ret = fallible
                .then(|| quote! { ::std::result::Result::Ok(#this_ident) })
                .unwrap_or_else(|| quote! { #this_ident });
            pm.custom_body = Some((
                String::from("Initable"),
                Box::new(parse_quote_spanned! { span => {
                    let #this_ident = #dest::#target(#(#args),*) #construct_try;
                    unsafe {
                        #go::gio::prelude::AsyncInitableExt::init_future(
                            &#this_ident,
                            #priority
                        )
                    }.await #init_try;
                    #ret
                } }),
            ));
            if has_priority {
                pm.sig.inputs.push(parse_quote! {
                    #priority_ident: #go::glib::Priority
                });
            }
        }
    }
    def.inner.public_methods.extend(new_async_constructors);
}
