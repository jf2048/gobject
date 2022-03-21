use crate::{property::Property, signal::Signal, virtual_method::VirtualMethod, util};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;

#[inline]
fn find_attr(
    attrs: &mut Vec<syn::Attribute>,
    name: &str,
    exists: bool,
    errors: &mut Vec<darling::Error>,
) -> Option<syn::Attribute> {
    let attr_index = attrs.iter().position(|a| a.path.is_ident(name));
    if let Some(attr_index) = attr_index {
        if exists {
            errors.push(
                syn::Error::new_spanned(
                    &attrs[attr_index],
                    format!("Only one #[{}] struct allowed in a class", name),
                )
                .into(),
            );
            None
        } else {
            Some(attrs.remove(attr_index))
        }
    } else {
        None
    }
}

pub struct TypeDefinition {
    pub span: proc_macro2::Span,
    pub name: Option<syn::Ident>,
    pub generics: Option<syn::Generics>,
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    pub virtual_methods: Vec<VirtualMethod>,
}

impl TypeDefinition {
    pub fn new(
        module: &mut syn::ItemMod,
        is_interface: bool,
        errors: &mut Vec<darling::Error>,
    ) -> Self {
        let mut def = Self {
            span: module.span(),
            name: None,
            generics: None,
            properties: Vec::new(),
            signals: Vec::new(),
            virtual_methods: Vec::new(),
        };
        if module.content.is_none() {
            errors.push(
                syn::Error::new_spanned(&module, "Module must have a body to use the class macro")
                    .into(),
            );
            return def;
        }
        let (_, items) = module.content.as_mut().unwrap();
        let mut struct_ = None;
        let mut impl_ = None;
        for item in items {
            match item {
                syn::Item::Struct(s) => {
                    if find_attr(&mut s.attrs, "properties", struct_.is_some(), errors).is_some() {
                        struct_ = Some(s);
                    }
                }
                syn::Item::Impl(i) => {
                    if find_attr(&mut i.attrs, "methods", impl_.is_some(), errors).is_some() {
                        if let Some((_, trait_, _)) = &i.trait_ {
                            errors.push(
                                syn::Error::new_spanned(
                                    &trait_,
                                    "Trait not allowed on #[methods] impl",
                                )
                                .into(),
                            );
                        }
                        impl_ = Some(i);
                    }
                }
                _ => {}
            }
        }
        let name = struct_.as_ref().map(|s| s.ident.clone());
        if let Some(struct_) = struct_ {}
        if let Some(impl_) = impl_ {
            def.generics = Some(impl_.generics.clone());
            def.signals
                .extend(Signal::many_from_items(&mut impl_.items, errors));
            def.virtual_methods
                .extend(VirtualMethod::many_from_items(&mut impl_.items, errors));
        }
        def
    }
    fn public_method_prototypes(&self) -> Vec<syn::Item> {
        let mut methods = vec![];
        methods
    }
    fn public_method_definitions(&self) -> Vec<syn::Item> {
        let mut methods = vec![];
        methods
    }
    pub fn public_methods(
        &self,
        name: &syn::Ident,
        trait_name: Option<&syn::Ident>,
        glib: &TokenStream
    ) -> Option<TokenStream> {
        let items = self.public_method_definitions();
        let type_ident = format_ident!("____Object");
        if let Some(generics) = self.generics.as_ref() {
            let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
            if let Some(trait_name) = trait_name {
                let mut generics = generics.clone();
                let param = util::parse(
                    quote! { #type_ident: #glib::IsA<#name #type_generics> },
                    &mut vec![],
                )
                .unwrap();
                generics.params.push(param);
                let (impl_generics, _, _) = generics.split_for_impl();
                let protos = self.public_method_prototypes();
                Some(quote! {
                    pub trait #trait_name: 'static {
                        #(#protos)*
                    }
                    impl #impl_generics #trait_name for #type_ident #where_clause {
                        #(#items)*
                    }
                })
            } else {
                Some(quote! {
                    impl #impl_generics #name #type_generics #where_clause {
                        #(#items)*
                    }
                })
            }
        } else {
            if let Some(trait_name) = trait_name {
                let protos = self.public_method_prototypes();
                Some(quote! {
                    pub trait #trait_name: 'static {
                        #(#protos)*
                    }
                    impl<#type_ident: #glib::IsA<#name>> #trait_name for #type_ident {
                        #(#items)*
                    }
                })
            } else {
                Some(quote! {
                    impl #name {
                        #(#items)*
                    }
                })
            }
        }
    }
    pub fn public_impls(
        &self,
        name: &syn::Ident,
        trait_name: Option<&syn::Ident>,
        glib: &TokenStream
    ) -> TokenStream {
        let public_methods = self.public_methods(name, trait_name, glib);
        quote! {
            #public_methods
        }
    }
}
