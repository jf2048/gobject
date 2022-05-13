use crate::{
    property::{Properties, Property},
    public_method::PublicMethod,
    signal::Signal,
    util::{self, Errors},
    virtual_method::VirtualMethod,
};
use heck::ToUpperCamelCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use std::{
    cell::RefCell,
    collections::{BTreeSet, HashMap},
};
use syn::{parse_quote, spanned::Spanned};

#[derive(Debug)]
pub struct TypeDefinition {
    pub module: syn::ItemMod,
    pub base: TypeBase,
    pub vis: syn::Visibility,
    pub inner_vis: syn::Visibility,
    pub concurrency: Concurrency,
    pub name: syn::Ident,
    pub crate_path: syn::Path,
    pub generics: syn::Generics,
    pub properties_item_index: Option<usize>,
    pub methods_item_indices: BTreeSet<usize>,
    pub properties: Vec<Property>,
    pub signals: Vec<Signal>,
    pub public_methods: Vec<PublicMethod>,
    pub virtual_methods: Vec<VirtualMethod>,
    custom_stmts: RefCell<HashMap<String, Vec<syn::Stmt>>>,
}

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum TypeMode {
    Subclass,
    Wrapper,
}

impl TypeMode {
    pub fn for_item_type(ty: &syn::Type) -> Option<Self> {
        match ty {
            syn::Type::Path(syn::TypePath { path, .. }) if path.segments.len() == 2 => {
                Some(Self::Wrapper)
            }
            syn::Type::Path(syn::TypePath { path, .. }) if path.segments.len() == 1 => {
                Some(Self::Subclass)
            }
            _ => None,
        }
    }
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

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum Concurrency {
    None,
    SendSync,
}

impl ToTokens for Concurrency {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::None => {}
            Self::SendSync => (quote! { + Send + Sync }).to_tokens(tokens),
        }
    }
}

#[inline]
fn type_ident(ty: &syn::Type) -> Option<&syn::Ident> {
    if let syn::Type::Path(syn::TypePath { path, .. }) = ty {
        if path.leading_colon.is_none() {
            if path.segments.len() == 1 {
                return Some(&path.segments[0].ident);
            }
            if path.segments.len() == 2 && path.segments[0].ident == "super" {
                return Some(&path.segments[1].ident);
            }
        }
    }
    None
}

impl TypeDefinition {
    pub fn parse(
        module: syn::ItemMod,
        base: TypeBase,
        mut name: Option<syn::Ident>,
        crate_path: syn::Path,
        errors: &Errors,
    ) -> Self {
        let mut item = syn::Item::Mod(module);
        super::closures(&mut item, &crate_path, errors);
        let module = match item {
            syn::Item::Mod(m) => m,
            _ => unreachable!(),
        };
        let default_name = syn::Ident::new(
            &module.ident.to_string().to_upper_camel_case(),
            module.ident.span(),
        );
        let mut def = Self {
            module,
            base,
            vis: syn::Visibility::Inherited,
            inner_vis: parse_quote! { pub(super) },
            concurrency: Concurrency::None,
            name: default_name,
            crate_path,
            generics: Default::default(),
            properties_item_index: None,
            methods_item_indices: BTreeSet::new(),
            properties: Vec::new(),
            signals: Vec::new(),
            public_methods: Vec::new(),
            virtual_methods: Vec::new(),
            custom_stmts: RefCell::new(HashMap::new()),
        };
        if def.module.content.is_none() {
            errors.push_spanned(
                &def.module,
                "Module must have a body to use the class macro",
            );
            return def;
        }
        let glib = def.glib();
        let (_, items) = def.module.content.as_mut().unwrap();
        let mut struct_ = None;
        let mut impls = Vec::new();
        let mut generics = None;
        if let Some(name) = &name {
            // if a name was provided, only use structs/impls matching the name
            for (index, item) in items.iter_mut().enumerate() {
                match item {
                    syn::Item::Struct(s) if struct_.is_none() && &s.ident == name => {
                        let attrs = util::extract_attrs(&mut s.attrs, "properties")
                            .unwrap_or_else(|| vec![parse_quote! { #[properties] }]);
                        struct_ = Some((index, attrs));
                    }
                    syn::Item::Impl(i)
                        if i.trait_.is_none() && type_ident(&*i.self_ty) == Some(name) =>
                    {
                        util::extract_attrs(&mut i.attrs, "methods");
                        impls.push(index);
                    }
                    _ => {}
                }
            }
        } else {
            {
                // search for a struct with a properties attribute
                // if not found then use the name from the first struct
                let mut first_struct = None;
                let mut struct_name = None;
                for (index, item) in items.iter_mut().enumerate() {
                    if let syn::Item::Struct(s) = item {
                        if let Some(attrs) = util::extract_attrs(&mut s.attrs, "properties") {
                            struct_ = Some((index, attrs));
                            struct_name = Some(s.ident.clone());
                            break;
                        } else if first_struct.is_none() {
                            first_struct = Some(index);
                            struct_name = Some(s.ident.clone());
                        }
                    }
                }
                if struct_.is_none() {
                    if let Some(index) = first_struct {
                        struct_ = Some((index, vec![parse_quote! { #[properties] }]));
                    }
                }
                if struct_.is_some() {
                    name = struct_name;
                }
            }
            if name.is_none() {
                // if no structs found, search for an impl with a methods attribute
                // if not found then use the name from the first impl with a suitable type
                let mut first_impl = None;
                let mut impl_name = None;
                for (index, item) in items.iter_mut().enumerate() {
                    if let syn::Item::Impl(i) = item {
                        if i.trait_.is_none() {
                            if let Some(ident) = type_ident(&*i.self_ty) {
                                if util::extract_attrs(&mut i.attrs, "methods").is_some() {
                                    impls.push(index);
                                    impl_name = Some(ident.clone());
                                    break;
                                } else if first_impl.is_none() {
                                    first_impl = Some(index);
                                    impl_name = Some(ident.clone());
                                }
                            }
                        }
                    }
                }
                if impls.is_empty() {
                    if let Some(index) = first_impl {
                        impls.push(index);
                    }
                }
                if !impls.is_empty() {
                    name = impl_name;
                }
            }
            if let Some(name) = &name {
                // if we got a name, then search for the rest of the impls matching that name
                // skip the first index that we might have found before
                let first = impls.first().cloned();
                for (index, item) in items.iter_mut().enumerate() {
                    if Some(index) == first {
                        continue;
                    }
                    if let syn::Item::Impl(i) = item {
                        if i.trait_.is_none() && type_ident(&*i.self_ty) == Some(name) {
                            util::extract_attrs(&mut i.attrs, "methods");
                            impls.push(index);
                        }
                    }
                }
            }
        }
        if let Some((index, attrs)) = struct_ {
            def.properties_item_index = Some(index);
            let struct_ = match &mut items[index] {
                syn::Item::Struct(s) => s,
                _ => unreachable!(),
            };
            match &struct_.vis {
                syn::Visibility::Inherited => {
                    struct_.vis = def.inner_vis.clone();
                }
                syn::Visibility::Restricted(syn::VisRestricted { path, .. })
                    if path.segments.len() == 1 && path.segments[0].ident == "self" =>
                {
                    struct_.vis = def.inner_vis.clone();
                }
                syn::Visibility::Restricted(syn::VisRestricted { path, .. })
                    if path.segments.len() > 1 && path.segments[0].ident == "super" =>
                {
                    let path = syn::Path {
                        leading_colon: None,
                        segments: FromIterator::from_iter(path.segments.iter().skip(1).cloned()),
                    };
                    def.vis = parse_quote! { pub(in #path) };
                    def.inner_vis = struct_.vis.clone();
                }
                _ => {
                    def.vis = struct_.vis.clone();
                    def.inner_vis = struct_.vis.clone();
                }
            };
            generics = Some(struct_.generics.clone());
            name = Some(struct_.ident.clone());
            let mut input: syn::DeriveInput = struct_.clone().into();
            for attr in attrs.into_iter().rev() {
                input.attrs.insert(0, attr);
            }
            let Properties {
                properties,
                mut fields,
                ..
            } = Properties::from_derive_input(&input, Some(base), errors);
            if base == TypeBase::Interface {
                match &mut fields {
                    syn::Fields::Named(f) => {
                        let fields: syn::FieldsNamed = parse_quote! {
                            { ____parent: #glib::gobject_ffi::GTypeInterface }
                        };
                        f.named.insert(0, fields.named.into_iter().next().unwrap());
                    }
                    syn::Fields::Unnamed(f) => {
                        let fields: syn::FieldsUnnamed = parse_quote! {
                            (#glib::gobject_ffi::GTypeInterface)
                        };
                        f.unnamed
                            .insert(0, fields.unnamed.into_iter().next().unwrap());
                    }
                    _ => {}
                };
            }
            struct_.fields = fields;
            def.properties.extend(properties);
        } else {
            def.vis = def.module.vis.clone();
            match &def.vis {
                syn::Visibility::Inherited => {}
                syn::Visibility::Restricted(syn::VisRestricted { path, .. })
                    if path.segments.len() == 1 && path.segments[0].ident == "self" => {}
                syn::Visibility::Restricted(syn::VisRestricted { path, .. })
                    if !path.segments.is_empty() && path.segments[0].ident == "super" =>
                {
                    def.inner_vis = parse_quote! { pub(in super::#path) }
                }
                _ => def.inner_vis = def.vis.clone(),
            }
        }
        for index in &impls {
            let impl_ = match &mut items[*index] {
                syn::Item::Impl(i) => i,
                _ => unreachable!(),
            };
            let mode = TypeMode::for_item_type(&*impl_.self_ty).unwrap_or_else(|| {
                unreachable!("Invalid type in mode: {}", impl_.self_ty.to_token_stream())
            });
            if generics.is_none() {
                generics = Some(impl_.generics.clone());
            }
            Signal::many_from_items(&mut impl_.items, base, mode, &mut def.signals, errors);
            def.public_methods.extend(PublicMethod::many_from_items(
                &mut impl_.items,
                base,
                mode,
                errors,
            ));
            def.virtual_methods.extend(VirtualMethod::many_from_items(
                &mut impl_.items,
                base,
                mode,
                errors,
            ));
        }
        Signal::validate_many(&def.signals, errors);
        if let Some(name) = name {
            def.name = name;
        }
        if let Some(generics) = generics {
            def.generics = generics;
        }
        def.methods_item_indices = impls.into_iter().collect();
        def
    }
    pub fn glib(&self) -> syn::Path {
        let go = &self.crate_path;
        parse_quote! { #go::glib }
    }
    pub fn type_(&self, from: TypeMode, to: TypeMode, ctx: TypeContext) -> syn::Type {
        use TypeBase::*;
        use TypeContext::*;
        use TypeMode::*;

        let name = &self.name;
        let glib = self.glib();
        let generics = &self.generics;

        let recv = match ctx {
            Internal => parse_quote! { Self },
            External => parse_quote! { #name #generics },
        };

        match (from, to, self.base) {
            (Subclass, Subclass, _) | (Wrapper, Wrapper, _) => recv,
            (Subclass, Wrapper, Class) => parse_quote! {
                <#recv as #glib::subclass::types::ObjectSubclass>::Type
            },
            (Subclass, Wrapper, Interface) => parse_quote! {
                super::#name #generics
            },
            (Wrapper, Subclass, Class) => parse_quote! {
                <#recv as #glib::object::ObjectSubclassIs>::Subclass
            },
            (Wrapper, Subclass, Interface) => parse_quote! {
                <#recv as #glib::object::ObjectType>::GlibClassType
            },
        }
    }
    pub fn properties_item(&self) -> Option<&syn::ItemStruct> {
        let index = self.properties_item_index?;
        match self.module.content.as_ref()?.1.get(index)? {
            syn::Item::Struct(s) => Some(s),
            _ => None,
        }
    }
    pub fn properties_item_mut(&mut self) -> Option<&mut syn::ItemStruct> {
        let index = self.properties_item_index?;
        match self.module.content.as_mut()?.1.get_mut(index)? {
            syn::Item::Struct(s) => Some(s),
            _ => None,
        }
    }
    pub fn methods_items(&self) -> impl Iterator<Item = &syn::ItemImpl> + '_ {
        self.methods_item_indices.iter().filter_map(|index| {
            let item = self.module.content.as_ref()?.1.get(*index)?;
            match item {
                syn::Item::Impl(i) => Some(i),
                _ => None,
            }
        })
    }
    pub fn methods_items_mut(&mut self) -> impl Iterator<Item = &mut syn::ItemImpl> + '_ {
        let indices = self.methods_item_indices.clone();
        self.ensure_items()
            .iter_mut()
            .enumerate()
            .filter_map(move |(index, item)| match item {
                syn::Item::Impl(i) if indices.contains(&index) => Some(i),
                _ => None,
            })
    }
    pub fn ensure_items(&mut self) -> &mut Vec<syn::Item> {
        &mut self.module.content.get_or_insert_with(Default::default).1
    }
    pub fn has_method(&self, mode: TypeMode, method: &str) -> bool {
        self.find_method(mode, method).is_some()
    }
    pub fn find_method(&self, mode: TypeMode, method: &str) -> Option<&syn::ImplItemMethod> {
        self.methods_items().find_map(|item| {
            let item_mode = TypeMode::for_item_type(&*item.self_ty)?;
            if item_mode != mode {
                return None;
            }
            item.items.iter().find_map(|item| match item {
                syn::ImplItem::Method(m) if m.sig.ident == method => Some(m),
                _ => None,
            })
        })
    }
    pub fn find_method_ident(
        &self,
        mode: TypeMode,
        ident: &syn::Ident,
    ) -> Option<&syn::ImplItemMethod> {
        self.methods_items().find_map(|item| {
            let item_mode = TypeMode::for_item_type(&*item.self_ty)?;
            if item_mode != mode {
                return None;
            }
            item.items.iter().find_map(|item| match item {
                syn::ImplItem::Method(m) if m.sig.ident == *ident => Some(m),
                _ => None,
            })
        })
    }
    #[inline]
    pub fn public_method(&self, mode: TypeMode, ident: &syn::Ident) -> Option<&PublicMethod> {
        self.public_methods
            .iter()
            .find(|pm| pm.matches(mode, ident))
    }
    #[inline]
    pub fn public_method_mut(
        &mut self,
        mode: TypeMode,
        ident: &syn::Ident,
    ) -> Option<&mut PublicMethod> {
        self.public_methods
            .iter_mut()
            .find(|pm| pm.matches(mode, ident))
    }
    pub fn add_custom_stmt(&self, name: &str, stmt: syn::Stmt) {
        let mut stmts = self.custom_stmts.borrow_mut();
        if let Some(stmts) = stmts.get_mut(name) {
            stmts.push(stmt);
        } else {
            stmts.insert(name.to_owned(), vec![stmt]);
        }
    }
    pub fn has_custom_stmts(&self, name: &str) -> bool {
        self.custom_stmts.borrow().contains_key(name)
    }

    pub fn custom_stmts_for(&self, name: &str) -> Option<TokenStream> {
        self.custom_stmts
            .borrow()
            .get(name)
            .map(|stmts| quote! { #({ #stmts };)* })
    }
    pub fn method_wrapper<F>(&self, name: &str, sig_func: F) -> Option<TokenStream>
    where
        F: FnOnce(&syn::Ident) -> syn::Signature,
    {
        let has_method = self.has_method(TypeMode::Subclass, name);
        let custom = self.custom_stmts_for(name);
        if !has_method && custom.is_none() {
            return None;
        }
        let ident = format_ident!("{}", name);
        let sig = sig_func(&ident);
        let call_user_method = has_method.then(|| {
            let input_names = sig.inputs.iter().map(|arg| match arg {
                syn::FnArg::Receiver(_) => quote_spanned! { Span::mixed_site() => self },
                syn::FnArg::Typed(arg) => match &*arg.pat {
                    syn::Pat::Ident(syn::PatIdent { ident, .. }) => {
                        quote_spanned! { Span::mixed_site() => #ident }
                    }
                    _ => unimplemented!(),
                },
            });
            quote! { Self::#ident(#(#input_names),*) }
        });
        Some(quote! {
            #sig {
                #custom
                #call_user_method
            }
        })
    }
    pub fn trait_head_with_params(
        &self,
        ty: &syn::Path,
        trait_: TokenStream,
        params: Option<impl IntoIterator<Item = syn::GenericParam>>,
    ) -> TokenStream {
        if let Some(params) = params {
            let (_, type_generics, where_clause) = self.generics.split_for_impl();
            let mut generics = self.generics.clone();
            generics.params.extend(params);
            let (impl_generics, _, _) = generics.split_for_impl();
            quote! {
                impl #impl_generics #trait_ for #ty #type_generics #where_clause
            }
        } else {
            let (impl_generics, type_generics, where_clause) = self.generics.split_for_impl();
            quote! {
                impl #impl_generics #trait_ for #ty #type_generics #where_clause
            }
        }
    }
    #[inline]
    pub fn trait_head(&self, ty: &syn::Path, trait_: TokenStream) -> TokenStream {
        self.trait_head_with_params(ty, trait_, None::<[syn::GenericParam; 0]>)
    }
    pub(crate) fn properties_method(&self) -> Option<TokenStream> {
        let has_method = self.has_method(TypeMode::Subclass, "properties");
        let custom = self.custom_stmts_for("properties");
        if self.properties.is_empty() && !has_method && custom.is_none() {
            return None;
        }
        let go = &self.crate_path;
        let glib = self.glib();
        let sub_ty = self.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        );
        let defs = self.properties.iter().map(|p| p.definition(go));
        let extra = has_method.then(|| {
            quote_spanned! { Span::mixed_site() =>
                properties.extend(#sub_ty::properties());
            }
        });
        let base_index_set = (self.base == TypeBase::Class
            && !self.properties.is_empty()
            && extra.is_some())
        .then(|| {
            quote_spanned! { Span::mixed_site() =>
                self::_GENERATED_PROPERTIES_BASE_INDEX.set(properties.len()).unwrap();
            }
        });
        Some(quote_spanned! { Span::mixed_site() =>
            fn properties() -> &'static [#glib::ParamSpec] {
                static PROPS: #glib::once_cell::sync::Lazy<::std::vec::Vec<#glib::ParamSpec>> =
                    #glib::once_cell::sync::Lazy::new(|| {
                        let mut properties = ::std::vec::Vec::<#glib::ParamSpec>::new();
                        #extra
                        #custom
                        #base_index_set
                        properties.extend([#(#defs),*]);
                        properties
                    });
                ::std::convert::AsRef::as_ref(::std::ops::Deref::deref(&PROPS))
            }
        })
    }
    pub(crate) fn signals_method(&self) -> Option<TokenStream> {
        let has_method = self.has_method(TypeMode::Subclass, "signals");
        let custom = self.custom_stmts_for("signals");
        if self.signals.is_empty() && !has_method && custom.is_none() {
            return None;
        }
        let glib = self.glib();
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        let sub_ty = self.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        );
        let defs = self
            .signals
            .iter()
            .map(|s| s.definition(&ty, &sub_ty, &glib));
        let extra = has_method.then(|| {
            quote_spanned! { Span::mixed_site() =>
                signals.extend(#sub_ty::signals());
            }
        });
        Some(quote_spanned! { Span::mixed_site() =>
            fn signals() -> &'static [#glib::subclass::Signal] {
                static SIGNALS: #glib::once_cell::sync::Lazy<::std::vec::Vec<#glib::subclass::Signal>> =
                    #glib::once_cell::sync::Lazy::new(|| {
                        let mut signals = ::std::vec::Vec::<#glib::subclass::Signal>::new();
                        #extra
                        #custom
                        signals.extend([#(#defs),*]);
                        signals
                    });
                ::std::convert::AsRef::as_ref(::std::ops::Deref::deref(&SIGNALS))
            }
        })
    }
    fn public_method_prototypes(&self) -> Vec<TokenStream> {
        let go = &self.crate_path;
        let glib = self.glib();
        self.properties
            .iter()
            .flat_map(|p| p.method_prototypes(self.concurrency, go))
            .chain(
                self.signals
                    .iter()
                    .flat_map(|s| s.method_prototypes(self.concurrency, &glib)),
            )
            .chain(
                self.public_methods
                    .iter()
                    .filter_map(|m| m.prototype(&glib)),
            )
            .chain(self.virtual_methods.iter().map(|m| m.prototype(&glib)))
            .collect()
    }
    pub(crate) fn method_path(&self, method: &str, from: TypeMode) -> syn::ExprPath {
        let glib = self.glib();
        let subclass_ty = self.type_(from, TypeMode::Subclass, TypeContext::External);
        let method = format_ident!("{}", method);
        match self.base {
            TypeBase::Class => parse_quote! {
                <#subclass_ty as #glib::subclass::object::ObjectImpl>::#method
            },
            TypeBase::Interface => parse_quote! {
                <#subclass_ty as #glib::subclass::prelude::ObjectInterface>::#method
            },
        }
    }
    pub(crate) fn public_method_definitions(
        &self,
        final_: bool,
    ) -> impl Iterator<Item = TokenStream> + '_ {
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);

        let properties = {
            let go = self.crate_path.clone();
            let ty = ty.clone();
            let properties_path = self.method_path("properties", TypeMode::Subclass);
            self.properties.iter().enumerate().flat_map(move |(i, p)| {
                p.method_definitions(i, &ty, self.concurrency, &properties_path, &go)
            })
        };
        let signals = {
            let glib = self.glib();
            self.signals
                .iter()
                .flat_map(move |s| s.method_definitions(self.concurrency, &glib))
        };
        let public_methods = {
            let glib = self.glib();
            let ty = ty.clone();
            let sub_ty = self.type_(
                TypeMode::Subclass,
                TypeMode::Subclass,
                TypeContext::External,
            );
            self.public_methods
                .iter()
                .filter_map(move |m| m.definition(&ty, &sub_ty, false, final_, &glib))
        };
        let virtual_methods = {
            let glib = self.glib();
            self.virtual_methods
                .iter()
                .map(move |m| m.definition(&ty, &glib))
        };
        properties
            .chain(signals)
            .chain(public_methods)
            .chain(virtual_methods)
    }
    pub(crate) fn public_methods(&self, trait_name: Option<&syn::Ident>) -> Option<TokenStream> {
        let go = &self.crate_path;
        let glib = self.glib();
        let final_ = trait_name.is_none();
        let mut items = self.public_method_definitions(final_).peekable();
        let name = &self.name;
        let type_ident = format_ident!("____Object");
        let vis = &self.inner_vis;
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        let sub_ty = self.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        );
        let mut statics = self
            .public_methods
            .iter()
            .filter_map(|m| m.definition(&ty, &sub_ty, true, final_, &glib))
            .peekable();
        let mut subclass_statics = self
            .public_methods
            .iter()
            .filter_map(|m| m.generated_definition(TypeMode::Subclass, &ty, &glib))
            .peekable();
        let mut wrapper_statics = self
            .public_methods
            .iter()
            .filter_map(|m| m.generated_definition(TypeMode::Wrapper, &ty, &glib))
            .peekable();
        let default_impls = self.public_methods.iter().filter_map(|m| {
            let def = m.default_impl(&ty, &sub_ty)?;
            let head = self.trait_head(
                &parse_quote! { super::#name },
                quote! { ::std::default::Default },
            );
            Some(quote_spanned! { m.sig.span() =>
                #head {
                    #def
                }
            })
        });
        let has_wrapper_statics = statics.peek().is_some() || wrapper_statics.peek().is_some();
        let has_subclass_statics = subclass_statics.peek().is_some();
        let async_trait = match self.concurrency {
            Concurrency::None => quote! { #[#go::async_trait::async_trait(?Send)] },
            Concurrency::SendSync => quote! { #[#go::async_trait::async_trait] },
        };
        let (impl_generics, type_generics, where_clause) = self.generics.split_for_impl();
        if let Some(trait_name) = trait_name {
            let items = items.peek().is_some().then(|| {
                let mut generics = self.generics.clone();
                let param = parse_quote! { #type_ident: #glib::IsA<super::#name #type_generics> };
                generics.params.push(param);
                let (impl_generics, _, _) = generics.split_for_impl();
                let protos = self.public_method_prototypes();
                quote! {
                    #async_trait
                    #vis trait #trait_name: 'static {
                        #(#protos;)*
                    }
                    #async_trait
                    impl #impl_generics #trait_name for #type_ident #where_clause {
                        #(#items)*
                    }
                }
            });
            let wrapper_statics = has_wrapper_statics.then(|| {
                quote! {
                    impl #impl_generics super::#name #type_generics #where_clause {
                        #(pub #statics)*
                        #(#wrapper_statics)*
                    }
                }
            });
            let subclass_statics = has_subclass_statics.then(|| {
                quote! {
                    impl #impl_generics #name #type_generics #where_clause {
                        #(#subclass_statics)*
                    }
                }
            });
            Some(quote! {
                #items
                #wrapper_statics
                #subclass_statics
                #(#default_impls)*
            })
        } else {
            Some(quote! {
                impl #impl_generics super::#name #type_generics #where_clause {
                    #(pub #items)*
                    #(pub #statics)*
                    #(#wrapper_statics)*
                }
                impl #impl_generics #name #type_generics #where_clause {
                    #(#subclass_statics)*
                }
                #(#default_impls)*
            })
        }
    }
    #[inline]
    fn default_vtable_assignments(&self, class_ident: &syn::Ident) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        let glib = self.glib();
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        let ty = parse_quote! { #ty };
        Some(FromIterator::from_iter(self.virtual_methods.iter().map(
            |m| m.set_default_trampoline(&self.name, &ty, class_ident, &glib),
        )))
    }
    pub(crate) fn type_init_body(&self, class_ident: &syn::Ident) -> Option<TokenStream> {
        let glib = self.glib();
        let wrapper_ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        let sub_ty = self.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        );
        let set_vtable = self.default_vtable_assignments(class_ident);
        let object_class = syn::Ident::new("____object_class", Span::mixed_site());
        let overrides = self
            .signals
            .iter()
            .filter_map(|signal| {
                signal.class_init_override(&wrapper_ty, &sub_ty, &object_class, &glib)
            })
            .collect::<Vec<_>>();
        if set_vtable.is_none() && overrides.is_empty() {
            return None;
        }
        let deref_class = (!overrides.is_empty()).then(|| {
            quote! {
                let #object_class = &mut *#class_ident;
                let #object_class = #glib::Class::upcast_ref_mut::<#glib::Object>(#object_class);
            }
        });
        Some(quote! {
            #set_vtable
            {
                #deref_class
                #(#overrides)*
            }
        })
    }
    #[inline]
    fn subclassed_vtable_assignments(
        &self,
        type_ident: &syn::Ident,
        class_ident: &syn::Ident,
        trait_name: &syn::Ident,
    ) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        let glib = self.glib();
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        let ty = parse_quote! { #ty };
        Some(FromIterator::from_iter(self.virtual_methods.iter().map(
            |m| m.set_subclassed_trampoline(&ty, trait_name, type_ident, class_ident, &glib),
        )))
    }
    pub(crate) fn child_type_init_body(
        &self,
        type_ident: &syn::Ident,
        class_ident: &syn::Ident,
        trait_name: &syn::Ident,
    ) -> Option<TokenStream> {
        self.subclassed_vtable_assignments(type_ident, class_ident, trait_name)
    }
    pub(crate) fn type_struct_fields(&self) -> Vec<TokenStream> {
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        self.virtual_methods
            .iter()
            .map(|method| method.vtable_field(&ty))
            .collect()
    }
    #[inline]
    fn impl_trait(
        &self,
        trait_name: &syn::Ident,
        ext_trait_name: &syn::Ident,
        parent_trait: Option<&syn::TypePath>,
    ) -> Option<TokenStream> {
        let glib = self.glib();
        let vis = &self.inner_vis;
        let parent_trait = parent_trait.map(|p| quote! { #p }).unwrap_or_else(|| {
            quote! { #glib::subclass::object::ObjectImpl }
        });
        let virtual_methods_default = self
            .virtual_methods
            .iter()
            .map(|m| m.default_definition(ext_trait_name, &glib));
        Some(quote! {
            #vis trait #trait_name: #parent_trait + 'static {
                #(#virtual_methods_default)*
            }
        })
    }
    #[inline]
    fn impl_ext_trait(
        &self,
        trait_name: &syn::Ident,
        ext_trait_name: &syn::Ident,
    ) -> Option<TokenStream> {
        if self.virtual_methods.is_empty() {
            return None;
        }
        let glib = self.glib();
        let ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());
        let vis = &self.inner_vis;
        let parent_method_protos = self
            .virtual_methods
            .iter()
            .map(|m| m.parent_prototype(&glib));
        let parent_method_definitions = self
            .virtual_methods
            .iter()
            .map(|m| m.parent_definition(&ty, &glib));
        Some(quote! {
            #vis trait #ext_trait_name: #glib::subclass::types::ObjectSubclass {
                #(#parent_method_protos;)*
            }
            impl<#type_ident: #trait_name> #ext_trait_name for #type_ident {
                #(#parent_method_definitions)*
            }
        })
    }
    pub(crate) fn virtual_traits(
        &self,
        trait_name: Option<&syn::Ident>,
        ext_trait_name: Option<&syn::Ident>,
        parent_trait: Option<&syn::TypePath>,
    ) -> Option<TokenStream> {
        let trait_name = trait_name?;
        let ext_trait_name = ext_trait_name?;
        let impl_trait = self.impl_trait(trait_name, ext_trait_name, parent_trait);
        let impl_ext_trait = self.impl_ext_trait(trait_name, ext_trait_name);
        Some(quote! {
            #impl_trait
            #impl_ext_trait
        })
    }
    fn private_methods(&self, mode: TypeMode) -> Vec<TokenStream> {
        let mut methods = Vec::new();
        let glib = self.glib();

        for signal in &self.signals {
            if let Some(chain) = signal.chain_definition(mode, &glib) {
                methods.push(chain);
            }
        }

        methods
    }
    pub(crate) fn extra_private_items(&self) -> Vec<TokenStream> {
        let mut items = Vec::new();

        let name = &self.name;
        let glib = self.glib();
        let wrapper_ty = self.type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);

        for signal in &self.signals {
            items.push(signal.signal_id_cell_definition(&wrapper_ty, &glib));
        }

        let private_methods = self.private_methods(TypeMode::Subclass);

        if !private_methods.is_empty() {
            let (impl_generics, type_generics, where_clause) = self.generics.split_for_impl();
            let head = quote! { impl #impl_generics #name #type_generics #where_clause };
            items.push(quote! {
                #head {
                    #(#private_methods)*
                }
            });
        }

        let private_methods = self.private_methods(TypeMode::Wrapper);

        if !private_methods.is_empty() {
            let (impl_generics, type_generics, where_clause) = self.generics.split_for_impl();
            let head = quote! { impl #impl_generics super::#name #type_generics #where_clause };
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
