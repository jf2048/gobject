use crate::{
    property::{Properties, Property},
    signal::Signal,
    util,
    virtual_method::VirtualMethod,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use std::collections::HashMap;

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
            util::push_error_spanned(
                errors,
                &attrs[attr_index],
                format!("Only one #[{}] struct allowed in a class", name),
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
    pub custom_methods: HashMap<String, syn::ImplItemMethod>,
}

impl TypeDefinition {
    pub fn new(
        mut module: syn::ItemMod,
        is_interface: bool,
        custom_methods: &[&str],
        errors: &mut Vec<darling::Error>,
    ) -> (Self, syn::ItemMod) {
        let mut def = Self {
            span: module.span(),
            name: None,
            generics: None,
            properties: Vec::new(),
            signals: Vec::new(),
            virtual_methods: Vec::new(),
            custom_methods: HashMap::new(),
        };
        if module.content.is_none() {
            util::push_error_spanned(
                errors,
                &module,
                "Module must have a body to use the class macro",
            );
            return (def, module);
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
                            util::push_error_spanned(
                                errors,
                                &trait_,
                                "Trait not allowed on #[methods] impl",
                            );
                        }
                        impl_ = Some(i);
                    }
                }
                _ => {}
            }
        }
        if let Some(struct_) = struct_ {
            def.generics = Some(struct_.generics.clone());
            def.name = Some(struct_.ident.clone());
            todo!("Reconcile final and abstract with class def");
            let Properties {
                final_,
                properties,
                fields,
            } = Properties::from_derive_input(&struct_.clone().into(), is_interface, errors);
            struct_.fields = fields;
            def.properties.extend(properties);
        }
        if let Some(impl_) = impl_ {
            if def.generics.is_none() {
                def.generics = Some(impl_.generics.clone());
            }
            def.signals
                .extend(Signal::many_from_items(&mut impl_.items, is_interface, errors));
            def.virtual_methods
                .extend(VirtualMethod::many_from_items(&mut impl_.items, errors));
        }
        todo!("custom methods");
        (def, module)
    }
    pub fn name(&self) -> Option<&syn::Ident> {
        self.name.as_ref()
    }
    pub fn properties_method(&self, method_name: &str, go: &syn::Ident) -> Option<TokenStream> {
        if self.properties.is_empty() {
            return None;
        }
        let glib = quote! { #go::glib };
        let defs = self.properties.iter().map(|p| p.definition(go));
        Some(quote! {
            fn #method_name() -> &'static [#glib::ParamSpec] {
                static PROPS: #glib::once_cell::sync::Lazy<::std::vec::Vec<#glib::ParamSpec>> =
                    #glib::once_cell::sync::Lazy::new(|| {
                        vec![#(#defs),*]
                    });
                ::std::convert::AsRef::as_ref(::std::ops::Deref::deref(&PROPS))
            }
        })
    }
    pub fn signals_method(&self, method_name: &str, glib: &TokenStream) -> Option<TokenStream> {
        if self.signals.is_empty() {
            return None;
        }
        let glib = quote! { #go::glib };
        let defs = self.signals.iter().map(|s| s.definition(glib));
        Some(quote! {
            fn #method_name() -> &'static [#glib::ParamSpec] {
                static SIGNALS: #glib::once_cell::sync::Lazy<::std::vec::Vec<#glib::subclass::Signal>> =
                    #glib::once_cell::sync::Lazy::new(|| {
                        vec![#(#defs),*]
                    });
                ::std::convert::AsRef::as_ref(::std::ops::Deref::deref(&SIGNALS))
            }
        })
    }
    pub fn has_custom_method(&self, method: &str) -> bool {
        self.custom_methods.contains_key(method)
    }
    pub fn custom_method(&self, method: &str) -> Option<TokenStream> {
        self.custom_methods.get(method).map(|m| quote! { #m })
    }
    pub fn custom_methods(&self, methods: &[&str]) -> TokenStream {
        FromIterator::from_iter(methods.iter().filter_map(|m| {
            self.custom_method(m)
        }))
    }
    fn public_method_prototypes(&self, go: &syn::Ident) -> Vec<TokenStream> {
        let mut protos = vec![];
        let glib = quote! { #go::glib };
        for prop in &self.properties {
            let ps = [
                prop.setter_prototype(go),
                prop.getter_prototype(go),
                prop.borrow_prototype(go),
                prop.pspec_prototype(&glib),
                prop.notify_prototype(),
                prop.connect_prototype(&glib),
            ];
            for proto in ps.into_iter().filter_map(|p| p) {
                protos.push(util::make_stmt(proto));
            }
        }
        for signal in &self.signals {
            let ps = [
                signal.emit_prototype(&glib),
                signal.connect_prototype(&glib),
            ];
            for proto in ps.into_iter().filter_map(|p| p) {
                protos.push(util::make_stmt(proto));
            }
        }
        for virtual_method in &self.virtual_methods {
            let proto = virtual_method.prototype();
            protos.push(util::make_stmt(proto));
        }
        protos
    }
    fn public_method_definitions(
        &self,
        ty: &syn::Type,
        is_interface: bool,
        go: &syn::Ident,
    ) -> Vec<TokenStream> {
        let mut methods = vec![];
        let glib = quote! { #go::glib };
        todo!("object type and properties path");
        let object_type = quote! {};
        let object_type = &object_type;
        let properties_path = quote! {};
        let properties_path = &properties_path;
        for (index, prop) in self.properties.iter().enumerate() {
            let defs = [
                prop.setter_definition(index, object_type, properties_path, go),
                prop.getter_definition(object_type, go),
                prop.borrow_definition(object_type, go),
                prop.pspec_definition(index, properties_path, &glib),
                prop.notify_definition(index, properties_path, &glib),
                prop.connect_definition(&glib),
            ];
            for method in defs.into_iter().filter_map(|d| d) {
                methods.push(util::make_stmt(method));
            }
        }
        for (index, signal) in self.signals.iter().enumerate() {
            let defs = [
                signal.emit_definition(index, ty, &glib),
                signal.connect_definition(index, ty, &glib),
            ];
            for method in defs.into_iter().filter_map(|d| d) {
                methods.push(util::make_stmt(method));
            }
        }
        for virtual_method in &self.virtual_methods {
            let method = virtual_method.definition(ty, is_interface, &glib);
            methods.push(util::make_stmt(method));
        }
        methods
    }
    pub fn public_methods(
        &self,
        name: &syn::Ident,
        ty: &syn::Type,
        trait_name: Option<&syn::Ident>,
        is_interface: bool,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        let glib = quote! { #go::glib };
        let items = self.public_method_definitions(ty, is_interface, go);
        if items.is_empty() {
            return None;
        }
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
                let protos = self.public_method_prototypes(go);
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
                let protos = self.public_method_prototypes(go);
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
    pub fn set_default_vtable(&self) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        Some(FromIterator::from_iter(self.virtual_methods.iter().map(|m| {
            m.set_default_trampoline()
        })))
    }
    pub fn set_subclassed_vtable(&self) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        Some(FromIterator::from_iter(self.virtual_methods.iter().map(|m| {
            m.set_subclassed_trampoline()
        })))
    }
    pub fn virtual_traits(
        &self,
        mod_name: &syn::Ident,
        name: &syn::Ident,
        parent_trait: &TokenStream,
        ty: &syn::Type,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        let trait_name = format_ident!("{}Impl", name);
        let ext_trait_name = format_ident!("{}ImplExt", name);
        let virtual_methods_default = self.virtual_methods.iter().map(|m| {
            m.default_definition(ty)
        });
        let parent_method_protos = self.virtual_methods.iter().map(|m| {
            m.parent_prototype(ty)
        });
        let parent_method_definitions = self.virtual_methods.iter().map(|m| {
            m.parent_definition(mod_name, name, ty)
        });
        Some(quote! {
            pub trait #trait_name: #parent_trait + 'static {
                #(#virtual_methods_default)*
            }
            pub trait #ext_trait_name: #glib::subclass::types::ObjectSubclass {
                #(#parent_method_protos)*
            }
            impl<T: #trait_name> #ext_trait_name for T {
                #(#parent_method_definitions)*
            }
        })
    }
}

impl Spanned for TypeDefinition {
    fn span(&self) -> proc_macro2::Span {
        self.span.clone()
    }
}
