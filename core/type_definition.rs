use crate::{
    property::{Properties, Property},
    signal::Signal,
    util,
    virtual_method::VirtualMethod,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::spanned::Spanned;

#[derive(Debug)]
pub struct TypeDefinitionParser {
    custom_methods: HashSet<String>,
}

impl TypeDefinitionParser {
    pub(crate) fn new() -> Self {
        Self {
            custom_methods: Default::default(),
        }
    }
    pub(crate) fn add_custom_method(&mut self, name: &str) -> &mut Self {
        self.custom_methods.insert(name.to_owned());
        self
    }
    pub(crate) fn parse(
        &self,
        module: syn::ItemMod,
        base: TypeBase,
        errors: &mut Vec<darling::Error>,
    ) -> TypeDefinition {
        let mut def = TypeDefinition {
            module,
            base,
            name: None,
            crate_ident: None,
            generics: None,
            properties: Vec::new(),
            signals: Vec::new(),
            virtual_methods: Vec::new(),
            custom_methods: HashMap::new(),
        };
        if def.module.content.is_none() {
            util::push_error_spanned(
                errors,
                &def.module,
                "Module must have a body to use the class macro",
            );
            return def;
        }
        let (_, items) = def.module.content.as_mut().unwrap();
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
            let Properties {
                properties,
                fields,
                ..
            } = Properties::from_derive_input(&struct_.clone().into(), base, true, errors);
            struct_.fields = fields;
            def.properties.extend(properties);
        }
        if let Some(impl_) = impl_ {
            if def.generics.is_none() {
                def.generics = Some(impl_.generics.clone());
            }
            def.signals
                .extend(Signal::many_from_items(&mut impl_.items, base, errors));
            def.virtual_methods
                .extend(VirtualMethod::many_from_items(&mut impl_.items, errors));

            extract_methods(
                &mut impl_.items,
                |m| self.custom_methods.contains(&m.sig.ident.to_string()),
                |m| def.custom_methods.insert(m.sig.ident.to_string(), m),
            );
        }
        def
    }
}

#[derive(Debug)]
pub struct TypeDefinition {
    pub module: syn::ItemMod,
    pub base: TypeBase,
    pub name: Option<syn::Ident>,
    pub crate_ident: Option<syn::Ident>,
    pub generics: Option<syn::Generics>,
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    pub virtual_methods: Vec<VirtualMethod>,
    pub custom_methods: HashMap<String, syn::ImplItemMethod>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum TypeMode {
    Subclass,
    Wrapper,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum TypeBase {
    Class,
    Interface,
}

macro_rules! unwrap_or_return {
    ($opt:expr, $ret:expr) => {
        match $opt {
            Some(val) => val,
            None => return $ret,
        }
    };
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
        Some(quote! { #go::glib })
    }
    pub fn type_(&self, from: TypeMode, to: TypeMode) -> Option<TokenStream> {
        use TypeBase::*;
        use TypeMode::*;

        let name = self.name.as_ref()?;
        let glib = self.glib()?;
        let generics = self.generics.as_ref();

        let recv = quote! { #name #generics };

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
        let go = self.crate_ident.as_ref()?;
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
        let ty = self.type_(TypeMode::Wrapper, TypeMode::Wrapper)?;
        let defs = self.signals.iter().map(|s| s.definition(&ty, &glib));
        Some(quote! {
            fn #method_name() -> &'static [#glib::subclass::Signal] {
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
        let go = unwrap_or_return!(self.crate_ident.as_ref(), protos);
        let glib = unwrap_or_return!(self.glib(), protos);
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
    pub(crate) fn method_path(&self, method: &str) -> Option<TokenStream> {
        let glib = self.glib()?;
        let subclass_ty = self.type_(TypeMode::Subclass, TypeMode::Subclass)?;
        Some(if self.has_custom_method(method) {
            let method = format_ident!("derived_{}", method);
            quote! { #subclass_ty::#method }
        } else {
            match self.base {
                TypeBase::Class => quote! {
                    <#subclass_ty as #glib::subclass::object::ObjectImpl>::#method
                },
                TypeBase::Interface => quote! {
                    <#subclass_ty as #glib::subclass::interface::ObjectInterface>::#method
                },
            }
        })
    }
    fn public_method_definitions(&self) -> Vec<TokenStream> {
        let mut methods = vec![];

        let go = unwrap_or_return!(self.crate_ident.as_ref(), methods);
        let glib = unwrap_or_return!(self.glib(), methods);
        let ty = unwrap_or_return!(self.type_(TypeMode::Wrapper, TypeMode::Wrapper), methods);
        let wrapper_ty =
            unwrap_or_return!(self.type_(TypeMode::Subclass, TypeMode::Wrapper), methods);
        let properties_path = unwrap_or_return!(self.method_path("properties"), methods);

        for (index, prop) in self.properties.iter().enumerate() {
            let defs = [
                prop.setter_definition(index, &ty, &properties_path, go),
                prop.getter_definition(&ty, go),
                prop.borrow_definition(&ty, go),
                prop.pspec_definition(index, &properties_path, &glib),
                prop.notify_definition(index, &properties_path, &glib),
                prop.connect_definition(&glib),
            ];
            for method in defs.into_iter().filter_map(|d| d) {
                methods.push(util::make_stmt(method));
            }
        }
        for (index, signal) in self.signals.iter().enumerate() {
            let defs = [
                signal.emit_definition(&glib),
                signal.connect_definition(&glib),
            ];
            for method in defs.into_iter().filter_map(|d| d) {
                methods.push(util::make_stmt(method));
            }
        }
        for virtual_method in &self.virtual_methods {
            let method = virtual_method.definition(&ty, self.base, &glib);
            methods.push(util::make_stmt(method));
        }
        methods
    }
    pub(crate) fn public_methods(&self, trait_name: Option<&syn::Ident>) -> Option<TokenStream> {
        let glib = self.glib()?;
        let items = self.public_method_definitions();
        if items.is_empty() {
            return None;
        }
        let name = self.name.as_ref()?;
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
                let protos = self.public_method_prototypes();
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
    #[inline]
    fn default_vtable_assignments(&self, class_ident: &syn::Ident) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        let glib = self.glib()?;
        let name = self.name.as_ref()?;
        let ty = self.type_(TypeMode::Wrapper, TypeMode::Wrapper)?;
        let ty = util::parse(ty, &mut vec![]).unwrap();
        Some(FromIterator::from_iter(self.virtual_methods.iter().map(
            |m| m.set_default_trampoline(name, &ty, class_ident, &glib),
        )))
    }
    pub(crate) fn type_init_body(&self, class_ident: &syn::Ident) -> Option<TokenStream> {
        let glib = self.glib()?;
        let wrapper_ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper)?;
        let set_vtable = self.default_vtable_assignments(&class_ident);
        let overrides = self
            .signals
            .iter()
            .filter_map(|signal| signal.class_init_override(&wrapper_ty, &class_ident, &glib))
            .collect::<Vec<_>>();
        if set_vtable.is_none() && overrides.is_empty() {
            return None;
        }
        Some(quote! {
            #(#overrides)*
            #set_vtable
        })
    }
    #[inline]
    fn subclassed_vtable_assignments(
        &self,
        type_ident: &syn::Ident,
        class_ident: &syn::Ident,
    ) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        let glib = self.glib()?;
        let name = self.name.as_ref()?;
        let ty = self.type_(TypeMode::Wrapper, TypeMode::Wrapper)?;
        let ty = util::parse(ty, &mut vec![]).unwrap();
        let trait_name = format_ident!("{}Impl", name);
        Some(FromIterator::from_iter(self.virtual_methods.iter().map(
            |m| m.set_subclassed_trampoline(&ty, &trait_name, type_ident, class_ident, &glib),
        )))
    }
    pub(crate) fn child_type_init_body(
        &self,
        type_ident: &syn::Ident,
        class_ident: &syn::Ident,
    ) -> Option<TokenStream> {
        self.subclassed_vtable_assignments(type_ident, class_ident)
    }
    pub(crate) fn type_struct_fields(&self) -> Vec<TokenStream> {
        let ty = unwrap_or_return!(self.type_(TypeMode::Wrapper, TypeMode::Wrapper), Vec::new());
        let ty = util::parse(ty, &mut vec![]).unwrap();
        self.virtual_methods
            .iter()
            .map(|method| method.vtable_field(&ty))
            .collect()
    }
    pub(crate) fn virtual_traits(&self, parent_trait: &TokenStream) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }

        let glib = self.glib()?;
        let name = self.name.as_ref()?;
        let ty = self.type_(TypeMode::Wrapper, TypeMode::Wrapper)?;
        let ty = util::parse(ty, &mut vec![]).unwrap();
        let trait_name = format_ident!("{}Impl", name);
        let ext_trait_name = format_ident!("{}ImplExt", name);
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());

        let virtual_methods_default = self
            .virtual_methods
            .iter()
            .map(|m| m.default_definition(&ty, &ext_trait_name));
        let parent_method_protos = self
            .virtual_methods
            .iter()
            .map(|m| m.parent_prototype(None, &ty));
        let parent_method_definitions = self
            .virtual_methods
            .iter()
            .map(|m| m.parent_definition(&self.module.ident, name, &ty, self.base, &glib));

        Some(quote! {
            pub(crate) trait #trait_name: #parent_trait + 'static {
                #(#virtual_methods_default)*
            }
            pub(crate) trait #ext_trait_name: #glib::subclass::types::ObjectSubclass {
                #(#parent_method_protos)*
            }
            impl<#type_ident: #trait_name> #ext_trait_name for #type_ident {
                #(#parent_method_definitions)*
            }
        })
    }
    fn private_methods(&self) -> Vec<TokenStream> {
        let mut methods = Vec::new();
        let glib = unwrap_or_return!(self.glib(), methods);

        for signal in &self.signals {
            if let Some(chain) = signal.chain_definition(&glib) {
                methods.push(chain);
            }
        }

        methods
    }
    pub(crate) fn extra_private_items(&self) -> Vec<TokenStream> {
        let mut items = Vec::new();

        let name = unwrap_or_return!(self.name.as_ref(), items);
        let glib = unwrap_or_return!(self.glib(), items);
        let wrapper_ty =
            unwrap_or_return!(self.type_(TypeMode::Subclass, TypeMode::Wrapper), items);

        for signal in &self.signals {
            items.push(signal.signal_id_cell_definition(&wrapper_ty, &glib));
        }

        let private_methods = self.private_methods();

        if !private_methods.is_empty() {
            let head = if let Some(generics) = self.generics.as_ref() {
                let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
                quote! { impl #impl_generics #name #type_generics #where_clause }
            } else {
                quote! { impl #name }
            };
            items.push(quote! {
                #head {
                    #(#private_methods)*
                }
            });
        }

        items
    }
}

impl Spanned for TypeDefinition {
    fn span(&self) -> proc_macro2::Span {
        self.module.span()
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
fn extract_methods<P, M, O>(items: &mut Vec<syn::ImplItem>, predicate: P, mut mapping: M) -> Vec<O>
where
    P: Fn(&mut syn::ImplItemMethod) -> bool,
    M: FnMut(syn::ImplItemMethod) -> O,
{
    let mut methods = Vec::new();
    let mut index = 0;
    while index < items.len() {
        let mut matched = false;
        if let syn::ImplItem::Method(method) = &mut items[index] {
            matched = predicate(method);
        }
        if matched {
            let item = items.remove(index);
            match item {
                syn::ImplItem::Method(method) => methods.push(mapping(method)),
                _ => unreachable!(),
            }
        } else {
            index += 1;
        }
    }
    methods
}
