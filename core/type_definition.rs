use crate::{
    property::{Properties, Property},
    signal::Signal,
    util,
    virtual_method::VirtualMethod,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::spanned::Spanned;

pub struct TypeDefinitionParser {
    custom_methods: HashSet<String>,
}

impl TypeDefinitionParser {
    pub(crate) fn new() -> Self {
        Self {
            custom_methods: Default::default()
        }
    }
    pub(crate) fn add_custom_method(&mut self, name: &str) -> &mut Self {
        self.custom_methods.insert(name.to_owned());
        self
    }
    pub(crate) fn parse(
        &self,
        module: &mut syn::ItemMod,
        is_interface: bool,
        errors: &mut Vec<darling::Error>,
    ) -> TypeDefinition {
        let mut def = TypeDefinition {
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
            def.signals.extend(Signal::many_from_items(
                &mut impl_.items,
                is_interface,
                errors,
            ));
            def.virtual_methods
                .extend(VirtualMethod::many_from_items(&mut impl_.items, errors));

            extract_methods(
                &mut impl_.items,
                errors,
                |m| self.custom_methods.contains(&m.sig.ident.to_string()),
                |m| def.custom_methods.insert(m.sig.ident.to_string(), m),
            );
        }
        def
    }
}

pub struct TypeDefinition {
    pub span: proc_macro2::Span,
    pub base: TypeBase,
    pub name: Option<syn::Ident>,
    pub crate_ident: Option<syn::Ident>,
    pub generics: Option<syn::Generics>,
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    pub virtual_methods: Vec<VirtualMethod>,
    pub custom_methods: HashMap<String, syn::ImplItemMethod>,
}

pub enum TypeMode {
    Subclass,
    Wrapper
}

pub enum TypeContext {
    Internal,
    External
}

pub enum TypeBase {
    Class,
    Interface
}

impl TypeDefinition {
    pub fn set_name(&mut self, name: syn::Ident) {
        self.name.replace(name);
    }
    pub fn set_crate_ident(&mut self, ident: syn::Ident) {
        self.crate_ident.replace(ident);
    }
    pub fn glib(&self) -> Option<TokenStream> {
        let go = self.crate_ident.as_ref()?;
        let glib = quote! { #go::glib };
    }
    pub fn type_(&self, from: TypeMode, to: TypeMode, context: TypeContext) -> Option<TokenStream> {
        use TypeBase::*;
        use TypeContext::*;
        use TypeMode::*;

        let name = self.name.as_ref()?;
        let glib = self.glib()?;
        let generics = self.generics.as_ref();

        let recv = match context {
            Internal => quote! { Self },
            External => quote! { #name #generics },
        };

        match (from, to, self.base) {
            (Subclass, Subclass, _) | (Wrapper, Wrapper, _) => Some(recv),
            (Subclass, Wrapper, Class) => Some(quote! {
                <#recv as #glib::subclass::types::ObjectSubclass>::Type
            }),
            (Subclass, Wrapper, Interface) => Some(quote! {
                <#recv as #glib::object::ObjectType>::GlibClassType
            }),
            (Wrapper, Subclass, Class) => Some(quote! {
                <#recv as #glib::Object::ObjectSubclassIs>::Subclass
            }),
            (Wrapper, Subclass, Interface) => Some(quote! {
                super::#name #generics
            }),
        }
    }
    pub(crate) fn properties_method(&self, method_name: &str) -> Option<TokenStream> {
        if self.properties.is_empty() {
            return None;
        }
        let glib = self.glib()?;
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
    pub(crate) fn signals_method(&self, method_name: &str) -> Option<TokenStream> {
        if self.signals.is_empty() {
            return None;
        }
        let glib = self.glib()?;
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
    pub(crate) fn has_custom_method(&self, method: &str) -> bool {
        self.custom_methods.contains_key(method)
    }
    pub(crate) fn custom_method(&self, method: &str) -> Option<TokenStream> {
        self.custom_methods.get(method).map(|m| quote! { #m })
    }
    pub(crate) fn custom_methods(&self, methods: &[&str]) -> TokenStream {
        FromIterator::from_iter(methods.iter().filter_map(|m| self.custom_method(m)))
    }
    fn public_method_prototypes(&self) -> Vec<TokenStream> {
        let mut protos = vec![];
        let (go, glib) = match (self.crate_ident.as_ref(), self.glib()) {
            (Some(go), Some(glib)) => (go, glib),
            _ => return protos,
        };
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
    ) -> Vec<TokenStream> {
        let mut methods = vec![];
        let (go, glib) = match (self.crate_ident.as_ref(), self.glib()) {
            (Some(go), Some(glib)) => (go, glib),
            _ => return methods,
        };
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
    pub(crate) fn public_methods(
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
                    pub(crate) trait #trait_name: 'static {
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
                    pub(crate) trait #trait_name: 'static {
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
    pub(crate) fn set_default_vtable(&self) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        Some(FromIterator::from_iter(
            self.virtual_methods
                .iter()
                .map(|m| m.set_default_trampoline()),
        ))
    }
    pub(crate) fn set_subclassed_vtable(&self) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        Some(FromIterator::from_iter(
            self.virtual_methods
                .iter()
                .map(|m| m.set_subclassed_trampoline()),
        ))
    }
    pub(crate) fn virtual_traits(
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
        let virtual_methods_default = self
            .virtual_methods
            .iter()
            .map(|m| m.default_definition(ty));
        let parent_method_protos = self.virtual_methods.iter().map(|m| m.parent_prototype(ty));
        let parent_method_definitions = self
            .virtual_methods
            .iter()
            .map(|m| m.parent_definition(mod_name, name, ty));
        Some(quote! {
            pub(crate) trait #trait_name: #parent_trait + 'static {
                #(#virtual_methods_default)*
            }
            pub(crate) trait #ext_trait_name: #glib::subclass::types::ObjectSubclass {
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
                format!("Only one #[{}] item allowed in a class", name),
            );
            None
        } else {
            Some(attrs.remove(attr_index))
        }
    } else {
        None
    }
}

#[inline]
fn extract_methods<P, M, O>(
    items: &mut Vec<syn::ImplItem>,
    errors: &mut Vec<darling::Error>,
    predicate: P,
    mapping: M,
) -> Vec<O>
where
    P: Fn(&mut syn::ImplItemMethod) -> bool,
    M: Fn(syn::ImplItemMethod) -> O
{
    let mut methods = Vec::new();
    let mut index = 0;
    while index < items.len() {
        let matched = false;
        if let syn::ImplItem::Method(method) = &mut items[index] {
            matched = predicate(method);
        }
        if matched {
            let item = items.remove(index);
            match item {
                syn::ImplItem::Method(method) => methods.push(mapping(method)),
                _ => unreachable!()
            }
        } else {
            index += 1;
        }
    }
    methods
}
