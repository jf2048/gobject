use crate::{
    property::{Properties, Property},
    public_method::PublicMethod,
    signal::Signal,
    util,
    virtual_method::VirtualMethod,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote};
use std::collections::{HashMap, HashSet};
use syn::{parse_quote, spanned::Spanned};

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
    pub fn parse(
        &self,
        module: syn::ItemMod,
        base: TypeBase,
        crate_ident: syn::Ident,
        errors: &mut Vec<darling::Error>,
    ) -> TypeDefinition {
        let mut syn_errors = vec![];
        let mut item = syn::Item::Mod(module);
        super::closures(&mut item, crate_ident.clone(), &mut syn_errors);
        errors.extend(syn_errors.into_iter().map(|e| e.into()));
        let module = match item {
            syn::Item::Mod(m) => m,
            _ => unreachable!(),
        };
        let mut def = TypeDefinition {
            module,
            base,
            name: None,
            crate_ident,
            generics: None,
            properties_item_index: None,
            methods_item_index: None,
            properties: Vec::new(),
            signals: Vec::new(),
            public_methods: Vec::new(),
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
        let glib = def.glib();
        let (_, items) = def.module.content.as_mut().unwrap();
        let mut struct_ = None;
        let mut impl_ = None;
        let mut first_struct = None;
        let mut first_impl = None;
        let mut struct_count = 0usize;
        let mut impl_count = 0usize;
        for (index, item) in items.iter_mut().enumerate() {
            match item {
                syn::Item::Struct(s) => {
                    if let Some(a) =
                        find_attr(&mut s.attrs, "properties", struct_.is_some(), errors)
                    {
                        struct_ = Some((index, a));
                    } else if first_struct.is_none() {
                        first_struct = Some(index);
                    }
                    struct_count += 1;
                }
                syn::Item::Impl(i) if i.trait_.is_none() => {
                    if find_attr(&mut i.attrs, "methods", impl_.is_some(), errors).is_some() {
                        if let Some((_, trait_, _)) = &i.trait_ {
                            util::push_error_spanned(
                                errors,
                                &trait_,
                                "Trait not allowed on #[methods] impl",
                            );
                        }
                        impl_ = Some(index);
                    } else if first_impl.is_none() {
                        first_impl = Some(index);
                    }
                    impl_count += 1;
                }
                _ => {}
            }
        }
        if struct_count == 1 && struct_.is_none() {
            if let Some(first_struct) = first_struct {
                struct_ = Some((first_struct, parse_quote! { #[properties] }));
            }
        }
        if impl_count == 1 && impl_.is_none() {
            // only use it if the names match
            if struct_
                .as_ref()
                .map(|(index, _)| {
                    let s = match &items[*index] {
                        syn::Item::Struct(s) => s,
                        _ => unreachable!(),
                    };
                    let impl_ = match &items[first_impl.unwrap()] {
                        syn::Item::Impl(i) => i,
                        _ => unreachable!(),
                    };
                    match impl_.self_ty.as_ref() {
                        syn::Type::Path(p) => p.path.is_ident(&s.ident),
                        _ => false,
                    }
                })
                .unwrap_or(true)
            {
                impl_ = first_impl;
            }
        }
        if let Some((index, attr)) = struct_ {
            def.properties_item_index = Some(index);
            let struct_ = match &mut items[index] {
                syn::Item::Struct(s) => s,
                _ => unreachable!(),
            };
            def.generics = Some(struct_.generics.clone());
            def.name = Some(struct_.ident.clone());
            let mut input: syn::DeriveInput = struct_.clone().into();
            input.attrs.insert(0, attr);
            let Properties {
                properties, mut fields, ..
            } = Properties::from_derive_input(&input, Some(base), errors);
            if base == TypeBase::Interface {
                match &mut fields {
                    syn::Fields::Named(f) => {
                        let fields: syn::FieldsNamed = parse_quote! {
                            { ____parent: #glib::gobject_ffi::GTypeInterface }
                        };
                        f.named.insert(0, fields.named.into_iter().next().unwrap());
                    },
                    syn::Fields::Unnamed(f) => {
                        let fields: syn::FieldsUnnamed = parse_quote! {
                            (#glib::gobject_ffi::GTypeInterface)
                        };
                        f.unnamed.insert(0, fields.unnamed.into_iter().next().unwrap());
                    },
                    _ => {},
                };
            }
            struct_.fields = fields;
            def.properties.extend(properties);
        }
        if let Some(index) = impl_ {
            def.methods_item_index = Some(index);
            let impl_ = match &mut items[index] {
                syn::Item::Impl(i) => i,
                _ => unreachable!(),
            };
            if def.generics.is_none() {
                def.generics = Some(impl_.generics.clone());
            }
            def.signals
                .extend(Signal::many_from_items(&mut impl_.items, base, errors));
            def.public_methods.extend(PublicMethod::many_from_items(
                &mut impl_.items,
                base,
                errors,
            ));
            def.virtual_methods.extend(VirtualMethod::many_from_items(
                &mut impl_.items,
                base,
                errors,
            ));

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
    pub crate_ident: syn::Ident,
    pub generics: Option<syn::Generics>,
    pub properties_item_index: Option<usize>,
    pub methods_item_index: Option<usize>,
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    pub public_methods: Vec<PublicMethod>,
    pub virtual_methods: Vec<VirtualMethod>,
    pub custom_methods: HashMap<String, syn::ImplItemMethod>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum TypeMode {
    Subclass,
    Wrapper,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum TypeContext {
    Internal,
    External,
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
    pub fn glib(&self) -> TokenStream {
        let go = &self.crate_ident;
        quote! { #go::glib }
    }
    pub fn type_(&self, from: TypeMode, to: TypeMode, ctx: TypeContext) -> Option<TokenStream> {
        use TypeBase::*;
        use TypeContext::*;
        use TypeMode::*;

        let name = self.name.as_ref()?;
        let glib = self.glib();
        let generics = self.generics.as_ref();

        let recv = match ctx {
            Internal => quote! { Self },
            External => quote! { #name #generics },
        };

        match (from, to, self.base) {
            (Subclass, Subclass, _) | (Wrapper, Wrapper, _) => Some(recv),
            (Subclass, Wrapper, Class) => Some(quote! {
                <#recv as #glib::subclass::types::ObjectSubclass>::Type
            }),
            (Subclass, Wrapper, Interface) => Some(quote! {
                super::#name #generics
            }),
            (Wrapper, Subclass, Class) => Some(quote! {
                <#recv as #glib::object::ObjectSubclassIs>::Subclass
            }),
            (Wrapper, Subclass, Interface) => Some(quote! {
                <#recv as #glib::object::ObjectType>::GlibClassType
            }),
        }
    }
    pub fn properties_item(&self) -> Option<&syn::ItemStruct> {
        let index = self.properties_item_index?;
        match self.module.content.as_ref()?.1.get(index)? {
            syn::Item::Struct(s) => Some(s),
            _ => None,
        }
    }
    pub fn methods_item(&self) -> Option<&syn::ItemImpl> {
        let index = self.methods_item_index?;
        match self.module.content.as_ref()?.1.get(index)? {
            syn::Item::Impl(i) => Some(i),
            _ => None,
        }
    }
    pub fn ensure_items(&mut self) -> &mut Vec<syn::Item> {
        &mut self.module.content.get_or_insert_with(Default::default).1
    }
    pub(crate) fn properties_method(&self, method_name: &str) -> Option<TokenStream> {
        if self.properties.is_empty() {
            return None;
        }
        let go = &self.crate_ident;
        let glib = self.glib();
        let defs = self.properties.iter().map(|p| p.definition(go));
        let method_name = format_ident!("{}", method_name);
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
    pub(crate) fn signals_method(&self) -> Option<TokenStream> {
        if self.signals.is_empty() {
            return None;
        }
        let glib = self.glib();
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External)?;
        let sub_ty = self.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        )?;
        let defs = self
            .signals
            .iter()
            .map(|s| s.definition(&ty, &sub_ty, &glib));
        Some(quote! {
            fn signals() -> &'static [#glib::subclass::Signal] {
                static SIGNALS: #glib::once_cell::sync::Lazy<::std::vec::Vec<#glib::subclass::Signal>> =
                    #glib::once_cell::sync::Lazy::new(|| {
                        vec![#(#defs),*]
                    });
                ::std::convert::AsRef::as_ref(::std::ops::Deref::deref(&SIGNALS))
            }
        })
    }
    pub(crate) fn derived_signals_method(&self) -> Option<TokenStream> {
        if self.signals.is_empty() {
            return None;
        }
        let glib = self.glib();
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External)?;
        let sub_ty = self.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        )?;
        let defs = self
            .signals
            .iter()
            .map(|s| s.definition(&ty, &sub_ty, &glib));
        Some(quote! {
            fn derived_signals() -> ::std::vec::Vec<#glib::subclass::Signal> {
                vec![#(#defs),*]
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
        let go = &self.crate_ident;
        let glib = self.glib();
        self.properties
            .iter()
            .map(|p| p.method_prototypes(go))
            .flatten()
            .chain(
                self.signals
                    .iter()
                    .map(|s| s.method_prototypes(&glib))
                    .flatten(),
            )
            .chain(self.public_methods.iter().map(|m| m.prototype()))
            .chain(self.virtual_methods.iter().map(|m| m.prototype(&glib)))
            .collect()
    }
    pub(crate) fn method_path(&self, method: &str, from: TypeMode) -> Option<TokenStream> {
        let glib = self.glib();
        let subclass_ty = self.type_(from, TypeMode::Subclass, TypeContext::External)?;
        Some(if self.has_custom_method(method) {
            let method = format_ident!("derived_{}", method);
            quote! { #subclass_ty::#method }
        } else {
            let method = format_ident!("{}", method);
            match self.base {
                TypeBase::Class => quote! {
                    <#subclass_ty as #glib::subclass::object::ObjectImpl>::#method
                },
                TypeBase::Interface => quote! {
                    <#subclass_ty as #glib::subclass::prelude::ObjectInterface>::#method
                },
            }
        })
    }
    fn public_method_definitions(&self) -> Vec<TokenStream> {
        let go = &self.crate_ident;
        let glib = self.glib();
        let ty = unwrap_or_return!(
            self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External),
            Vec::new()
        );
        let sub_ty = unwrap_or_return!(
            self.type_(
                TypeMode::Subclass,
                TypeMode::Subclass,
                TypeContext::External
            ),
            Vec::new()
        );
        let properties_path = unwrap_or_return!(
            self.method_path("properties", TypeMode::Subclass),
            Vec::new()
        );

        self.properties
            .iter()
            .enumerate()
            .map(|(i, p)| p.method_definitions(i, &ty, &properties_path, go))
            .flatten()
            .chain(
                self.signals
                    .iter()
                    .map(|s| s.method_definitions(&glib))
                    .flatten(),
            )
            .chain(
                self.public_methods
                    .iter()
                    .map(|m| m.definition(&ty, &sub_ty, &glib)),
            )
            .chain(
                self.virtual_methods
                    .iter()
                    .map(|m| m.definition(&ty, &glib)),
            )
            .collect()
    }
    pub(crate) fn public_methods(&self, trait_name: Option<&syn::Ident>) -> Option<TokenStream> {
        let glib = self.glib();
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
                let param = parse_quote! { #type_ident: #glib::IsA<super::#name #type_generics> };
                generics.params.push(param);
                let (impl_generics, _, _) = generics.split_for_impl();
                let protos = self.public_method_prototypes();
                Some(quote! {
                    pub trait #trait_name: 'static {
                        #(#protos;)*
                    }
                    impl #impl_generics #trait_name for #type_ident #where_clause {
                        #(#items)*
                    }
                })
            } else {
                Some(quote! {
                    impl #impl_generics super::#name #type_generics #where_clause {
                        #(pub #items)*
                    }
                })
            }
        } else {
            if let Some(trait_name) = trait_name {
                let protos = self.public_method_prototypes();
                Some(quote! {
                    pub trait #trait_name: 'static {
                        #(#protos;)*
                    }
                    impl<#type_ident: #glib::IsA<super::#name>> #trait_name for #type_ident {
                        #(#items)*
                    }
                })
            } else {
                Some(quote! {
                    impl super::#name {
                        #(pub #items)*
                    }
                })
            }
        }
    }
    #[inline]
    fn default_vtable_assignments(&self, class_ident: &TokenStream) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        let glib = self.glib();
        let name = self.name.as_ref()?;
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External)?;
        let ty = parse_quote! { #ty };
        Some(FromIterator::from_iter(self.virtual_methods.iter().map(
            |m| m.set_default_trampoline(name, &ty, class_ident, &glib),
        )))
    }
    pub(crate) fn type_init_body(&self, class_ident: &TokenStream) -> Option<TokenStream> {
        let glib = self.glib();
        let wrapper_ty =
            self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External)?;
        let sub_ty = self.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        )?;
        let set_vtable = self.default_vtable_assignments(class_ident);
        let overrides = self
            .signals
            .iter()
            .filter_map(|signal| {
                signal.class_init_override(&wrapper_ty, &sub_ty, class_ident, &glib)
            })
            .collect::<Vec<_>>();
        if set_vtable.is_none() && overrides.is_empty() {
            return None;
        }
        let deref_class = (!overrides.is_empty()).then(|| {
            quote! {
                let #class_ident = &mut *#class_ident;
                let #class_ident = #glib::Class::upcast_ref_mut::<#glib::Object>(#class_ident);
            }
        });
        Some(quote! {
            #set_vtable
            #deref_class
            #(#overrides)*
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
        let glib = self.glib();
        let name = self.name.as_ref()?;
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External)?;
        let ty = parse_quote! { #ty };
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
        let ty = unwrap_or_return!(
            self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External),
            Vec::new()
        );
        let ty = parse_quote! { #ty };
        self.virtual_methods
            .iter()
            .map(|method| method.vtable_field(&ty))
            .collect()
    }
    pub(crate) fn virtual_traits(&self, parent_trait: Option<TokenStream>) -> Option<TokenStream> {
        let glib = self.glib();
        let name = self.name.as_ref()?;
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External)?;
        let ty = parse_quote! { #ty };
        let trait_name = format_ident!("{}Impl", name);
        let ext_trait_name = format_ident!("{}ImplExt", name);
        let parent_trait = parent_trait.unwrap_or_else(|| {
            quote! {
                #glib::subclass::object::ObjectImpl
            }
        });
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());

        let virtual_methods_default = self
            .virtual_methods
            .iter()
            .map(|m| m.default_definition(&ext_trait_name, &glib));
        let ext_trait = (!self.virtual_methods.is_empty()).then(|| {
            let parent_method_protos = self
                .virtual_methods
                .iter()
                .map(|m| m.parent_prototype(&glib));
            let parent_method_definitions = self
                .virtual_methods
                .iter()
                .map(|m| m.parent_definition(name, &ty, &glib));
            quote! {
                pub trait #ext_trait_name: #glib::subclass::types::ObjectSubclass {
                    #(#parent_method_protos;)*
                }
                impl<#type_ident: #trait_name> #ext_trait_name for #type_ident {
                    #(#parent_method_definitions)*
                }
            }
        });

        Some(quote! {
            pub trait #trait_name: #parent_trait + 'static {
                #(#virtual_methods_default)*
            }
            #ext_trait
        })
    }
    fn private_methods(&self) -> Vec<TokenStream> {
        let mut methods = Vec::new();
        let glib = self.glib();

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
        let glib = self.glib();
        let wrapper_ty = unwrap_or_return!(
            self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External),
            items
        );

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
