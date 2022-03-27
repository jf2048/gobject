use crate::util;
use darling::{
    util::{Flag, SpannedValue},
    FromDeriveInput, FromField, FromMeta,
};
use heck::{ToKebabCase, ToSnakeCase};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use std::collections::{HashMap, HashSet};
use syn::spanned::Spanned;

#[derive(FromDeriveInput)]
#[darling(default, attributes(properties))]
pub(crate) struct PropertiesAttrs {
    pod: Flag,
    #[darling(rename = "final")]
    final_: Flag,
    data: darling::ast::Data<darling::util::Ignored, PropertyAttrs>,
}

impl Default for PropertiesAttrs {
    fn default() -> Self {
        Self {
            pod: Default::default(),
            final_: Default::default(),
            data: darling::ast::Data::empty_from(&syn::Data::Struct(syn::DataStruct {
                struct_token: Default::default(),
                fields: syn::Fields::Unit,
                semi_token: Some(Default::default()),
            })),
        }
    }
}

#[derive(Default, FromField)]
#[darling(default, attributes(property))]
struct PropertyAttrs {
    ident: Option<syn::Ident>,
    attrs: Vec<syn::Attribute>,
    skip: SpannedValue<Flag>,
    get: SpannedValue<Option<PropertyPermission>>,
    set: SpannedValue<Option<PropertyPermission>>,
    borrow: SpannedValue<Flag>,
    construct: SpannedValue<Option<bool>>,
    construct_only: SpannedValue<Option<bool>>,
    lax_validation: SpannedValue<Option<bool>>,
    explicit_notify: SpannedValue<Option<bool>>,
    deprecated: SpannedValue<Option<bool>>,
    notify: Option<bool>,
    connect_notify: Option<bool>,
    name: Option<syn::LitStr>,
    nick: Option<syn::LitStr>,
    blurb: Option<syn::LitStr>,
    #[darling(rename = "enum")]
    enum_: SpannedValue<Flag>,
    flags: SpannedValue<Flag>,
    boxed: SpannedValue<Flag>,
    object: SpannedValue<Flag>,
    computed: SpannedValue<Flag>,
    storage: Option<SpannedValue<PropertyStorageAttr>>,
    #[darling(rename = "abstract")]
    abstract_: SpannedValue<Flag>,
    override_class: Option<syn::Path>,
    override_iface: Option<syn::Path>,
    builder: SpannedValue<HashMap<syn::Ident, syn::Lit>>,
}

impl PropertyAttrs {
    fn name(&self, index: usize) -> PropertyName {
        if let Some(name) = &self.name {
            PropertyName::Custom(name.clone())
        } else if let Some(ident) = &self.ident {
            PropertyName::Field(ident.clone())
        } else {
            PropertyName::Field(format_ident!("UNNAMED{}", index))
        }
    }
    fn special_type(&self) -> PropertyType {
        if self.enum_.is_some() {
            PropertyType::Enum
        } else if self.flags.is_some() {
            PropertyType::Flags
        } else if self.boxed.is_some() {
            PropertyType::Boxed
        } else if self.object.is_some() {
            PropertyType::Object
        } else {
            PropertyType::Unspecified
        }
    }
    fn storage(&self, index: usize, iface: bool) -> PropertyStorage {
        if iface {
            PropertyStorage::InterfaceAbstract
        } else if self.computed.is_some() {
            PropertyStorage::Computed
        } else if self.abstract_.is_some() {
            PropertyStorage::Abstract
        } else if let Some(storage) = &self.storage {
            PropertyStorage::Delegate(Box::new(storage.0.clone()))
        } else if let Some(ident) = &self.ident {
            PropertyStorage::NamedField(ident.clone())
        } else {
            PropertyStorage::UnnamedField(index)
        }
    }
    fn override_(&self) -> Option<PropertyOverride> {
        if let Some(path) = &self.override_class {
            Some(PropertyOverride::Class(path.clone()))
        } else if let Some(path) = &self.override_iface {
            Some(PropertyOverride::Interface(path.clone()))
        } else {
            None
        }
    }
    fn flags(&self, pod: bool) -> PropertyFlags {
        let mut flags = PropertyFlags::empty();
        flags.set(
            PropertyFlags::READABLE,
            (*self.get).as_ref().map(|g| g.is_allowed()).unwrap_or(pod),
        );
        flags.set(
            PropertyFlags::WRITABLE,
            (*self.set).as_ref().map(|s| s.is_allowed()).unwrap_or(pod),
        );
        flags.set(PropertyFlags::CONSTRUCT, self.construct.unwrap_or(false));
        flags.set(
            PropertyFlags::CONSTRUCT_ONLY,
            self.construct_only.unwrap_or(false),
        );
        flags.set(
            PropertyFlags::LAX_VALIDATION,
            self.lax_validation.unwrap_or(false),
        );
        flags.set(
            PropertyFlags::EXPLICIT_NOTIFY,
            self.explicit_notify.unwrap_or(false),
        );
        flags.set(PropertyFlags::DEPRECATED, self.deprecated.unwrap_or(false));
        flags
    }
    fn normalize(&mut self, index: usize, pod: bool) {
        if pod {
            if self.get.is_none() {
                self.get = SpannedValue::new(Some(PropertyPermission::Allow), self.ident.span());
            }
            if self.set.is_none() {
                self.set = SpannedValue::new(Some(PropertyPermission::Allow), self.ident.span());
            }
            if self.override_().is_none() {
                if self.lax_validation.is_none() {
                    self.lax_validation = SpannedValue::new(Some(true), self.ident.span());
                }
                if self.explicit_notify.is_none() {
                    self.explicit_notify = SpannedValue::new(Some(true), self.ident.span());
                }
            }
        } else {
            if self
                .attrs
                .iter()
                .find(|a| a.path.is_ident("property"))
                .is_none()
            {
                self.skip = SpannedValue::new(Flag::present(), Span::call_site());
            }
        }
        let name = self.name(index).to_string();
        let computed = self.computed.is_some();
        if let Some(get) = self.get.as_mut() {
            get.normalize(computed, || format!("Self::Type::{}", name));
        }
        if let Some(set) = self.set.as_mut() {
            set.normalize(computed, || format!("Self::Type::set_{}", name));
        }
    }
    fn validate(
        &self,
        field: &syn::Field,
        pod: bool,
        iface: bool,
        errors: &mut Vec<darling::Error>,
    ) {
        use crate::validations::*;

        if self.skip.is_none() && self.ident.is_none() && self.name.is_none() {
            util::push_error_spanned(
                errors,
                field,
                "#[property(name = \"...\")] required for tuple struct properties",
            );
        }

        let name = self.name(0);
        if !util::is_valid_name(&name.to_string()) {
            util::push_error(
                errors,
                name.span(),
                format!(
                    "Invalid property name '{}'. Property names must start with an ASCII letter and only contain ASCII letters, numbers, '-' or '_'",
                    name
                )
            );
        }

        if self.skip.is_none()
            && !(*self.get)
                .as_ref()
                .map(|p| p.is_allowed())
                .unwrap_or(false)
            && !(*self.set)
                .as_ref()
                .map(|p| p.is_allowed())
                .unwrap_or(false)
        {
            util::push_error_spanned(errors, field, "Property must be readable or writable");
        }

        let interface = ("interface", iface.then(|| self.borrow.span()));
        let enum_ = ("enum", check_flag(&self.enum_));
        let flags = ("flags", check_flag(&self.flags));
        let boxed = ("boxed", check_flag(&self.boxed));
        let object = ("object", check_flag(&self.object));
        let override_class = (
            "override_class",
            self.override_class.as_ref().map(|o| o.span()),
        );
        let override_iface = (
            "override_iface",
            self.override_iface.as_ref().map(|o| o.span()),
        );
        let storage = ("storage", self.storage.as_ref().map(|s| s.0.span()));
        let abstract_ = ("abstract", check_flag(&self.abstract_));
        let computed = ("computed", check_flag(&self.computed));
        let write_only = (
            "write-only",
            (*self.get)
                .as_ref()
                .map(|a| (!a.is_allowed()).then(|| self.get.span()))
                .unwrap_or_else(|| (!pod).then(|| self.ident.span())),
        );
        let custom_getter = (
            "get = \"()\"",
            (*self.get).as_ref().and_then(|a| {
                matches!(a, PropertyPermission::AllowNoMethod).then(|| self.get.span())
            }),
        );
        let custom_setter = (
            "set = \"()\"",
            (*self.set).as_ref().and_then(|a| {
                matches!(a, PropertyPermission::AllowNoMethod).then(|| self.set.span())
            }),
        );
        let construct = ("construct", check_bool(&self.construct));
        let construct_only = ("construct_only", check_bool(&self.construct_only));
        let lax_validation = ("lax_validation", check_bool(&self.lax_validation));
        let explicit_notify = ("explicit_notify", check_bool(&self.explicit_notify));
        let deprecated = ("deprecated", check_bool(&self.deprecated));
        let nick = ("nick", self.nick.as_ref().map(|n| n.span()));
        let blurb = ("blurb", self.blurb.as_ref().map(|b| b.span()));
        let builder = (
            "builder",
            (!self.builder.is_empty()).then(|| self.builder.span()),
        );

        only_one([&enum_, &flags, &boxed, &object], errors);
        only_one([&override_class, &override_iface], errors);
        only_one([&storage, &abstract_, &computed], errors);

        if interface.1.is_some() {
            disallow(
                "interface property",
                [
                    &storage,
                    &abstract_,
                    &computed,
                    &custom_getter,
                    &custom_setter,
                ],
                errors,
            );
        }

        if self.override_class.is_some() || self.override_iface.is_some() {
            disallow(
                "overridden property",
                [
                    &storage,
                    &abstract_,
                    &custom_getter,
                    &custom_setter,
                    &nick,
                    &blurb,
                    &builder,
                    &construct,
                    &construct_only,
                    &lax_validation,
                    &explicit_notify,
                    &deprecated,
                ],
                errors,
            );
        }

        if matches!(*self.set, Some(PropertyPermission::Deny)) {
            disallow("read-only property", [&construct, &construct_only], errors);
        }

        if self.borrow.is_some() {
            let checks = [&interface, &write_only, &abstract_, &computed];
            for (attr_name, fail_span) in checks {
                if fail_span.is_some() {
                    util::push_error(
                        errors,
                        self.borrow.span(),
                        format!("`borrow` not allowed on {} property", attr_name),
                    );
                }
            }
        }
    }
}

struct PropertyStorageAttr(syn::Expr);

impl FromMeta for PropertyStorageAttr {
    fn from_string(value: &str) -> darling::Result<Self> {
        Ok(Self(syn::parse_str(&value)?))
    }
}

#[derive(PartialEq)]
enum PropertyPermission {
    Deny,
    Allow,
    AllowNoMethod,
    AllowCustom(syn::Path),
}

impl Default for PropertyPermission {
    fn default() -> Self {
        Self::Deny
    }
}

impl FromMeta for PropertyPermission {
    fn from_word() -> darling::Result<Self> {
        Ok(Self::Allow)
    }
    fn from_bool(allow: bool) -> darling::Result<Self> {
        Ok(if allow { Self::Allow } else { Self::Deny })
    }
    fn from_string(value: &str) -> darling::Result<Self> {
        if value == "()" {
            return Ok(Self::AllowNoMethod);
        }
        Ok(Self::AllowCustom(syn::parse_str(&value)?))
    }
}

impl PropertyPermission {
    fn normalize<F: FnOnce() -> String>(&mut self, computed: bool, make_name: F) {
        if computed && matches!(self, Self::Allow) {
            *self = Self::AllowCustom(syn::parse_str("_").unwrap());
        }
        if let Self::AllowCustom(path) = self {
            if path.is_ident("_") {
                *path = syn::parse_str(&make_name()).unwrap();
            }
        }
    }
    fn is_allowed(&self) -> bool {
        !matches!(self, Self::Deny)
    }
}

bitflags::bitflags! {
    struct PropertyFlags: u32 {
        const READABLE        = 1 << 0;
        const WRITABLE        = 1 << 1;
        const CONSTRUCT       = 1 << 2;
        const CONSTRUCT_ONLY  = 1 << 3;
        const LAX_VALIDATION  = 1 << 4;
        const EXPLICIT_NOTIFY = 1 << 30;
        const DEPRECATED      = 1 << 31;
    }
}

impl PropertyFlags {
    fn tokens(&self, glib: &TokenStream) -> TokenStream {
        let count = Self::empty().bits().leading_zeros() - Self::all().bits().leading_zeros();
        let mut flags = vec![];
        for i in 0..count {
            if let Some(flag) = Self::from_bits(1 << i) {
                if self.contains(flag) {
                    let flag = format!("{:?}", flag);
                    let flag = format_ident!("{}", flag);
                    flags.push(quote! { #glib::ParamFlags::#flag });
                }
            }
        }
        if flags.is_empty() {
            quote! { #glib::ParamFlags::empty() }
        } else {
            quote! { #(#flags)|* }
        }
    }
}

enum PropertyType {
    Unspecified,
    Enum,
    Flags,
    Boxed,
    Object,
}

impl PropertyType {
    fn builder(&self, name: &str, ty: &syn::Type, go: &syn::Ident) -> TokenStream {
        let glib = quote! { #go::glib };
        let pspec_type = match self {
            Self::Unspecified => {
                return quote_spanned! { ty.span() =>
                    <#ty as #go::ParamSpecBuildable>::builder(#name)
                }
            }
            Self::Enum => format_ident!("ParamSpecEnum"),
            Self::Flags => format_ident!("ParamSpecFlags"),
            Self::Boxed => format_ident!("ParamSpecBoxed"),
            Self::Object => format_ident!("ParamSpecObject"),
        };
        quote_spanned! { ty.span() =>
            #glib::#pspec_type::builder(
                #name,
                <<#ty as #glib::value::ValueType>::Type as #glib::StaticType>::static_type(),
            )
        }
    }
}

enum PropertyStorage {
    NamedField(syn::Ident),
    UnnamedField(usize),
    InterfaceAbstract,
    Abstract,
    Computed,
    Delegate(Box<syn::Expr>),
}

impl PropertyStorage {
    fn has_field(&self) -> bool {
        matches!(
            self,
            PropertyStorage::NamedField(_) | PropertyStorage::UnnamedField(_)
        )
    }
}

enum PropertyName {
    Field(syn::Ident),
    Custom(syn::LitStr),
}

impl Spanned for PropertyName {
    fn span(&self) -> Span {
        match self {
            PropertyName::Field(name) => name.span(),
            PropertyName::Custom(name) => name.span(),
        }
    }
}

impl std::fmt::Display for PropertyName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PropertyName::Field(name) => name.to_string().to_kebab_case().fmt(f),
            PropertyName::Custom(name) => name.value().fmt(f),
        }
    }
}

enum PropertyOverride {
    Interface(syn::Path),
    Class(syn::Path),
}

impl PropertyOverride {
    fn pspec(&self, name: &str, glib: &TokenStream) -> TokenStream {
        match self {
            PropertyOverride::Interface(target) => quote! {
                #glib::ParamSpecOverride::for_interface::<#target>(#name)
            },
            PropertyOverride::Class(target) => quote! {
                #glib::ParamSpecOverride::for_class::<#target>(#name)
            },
        }
    }
}

pub(crate) struct Properties {
    pub(crate) final_: bool,
    pub(crate) properties: Vec<Property>,
    pub(crate) fields: syn::Fields,
}

impl Default for Properties {
    fn default() -> Self {
        Self {
            final_: false,
            properties: Vec::new(),
            fields: syn::Fields::Unit,
        }
    }
}

impl Properties {
    pub(crate) fn from_derive_input(
        input: &syn::DeriveInput,
        iface: bool,
        errors: &mut Vec<darling::Error>,
    ) -> Self {
        let PropertiesAttrs { pod, final_, data } = match PropertiesAttrs::from_derive_input(&input)
        {
            Ok(attrs) => attrs,
            Err(e) => {
                errors.push(e.into());
                Default::default()
            }
        };
        let pod = pod.is_some();
        let final_ = final_.is_some();
        let data = data.take_struct().map(|s| s.fields).unwrap_or_default();

        let mut fields = match &input.data {
            syn::Data::Struct(syn::DataStruct { fields: fs, .. }) => fs.clone(),
            _ => return Default::default(),
        };

        let mut prop_names = HashSet::new();
        let mut properties = vec![];
        let mut fs = vec![];
        for (index, (attrs, field)) in std::iter::zip(data, fields.iter()).enumerate() {
            let mut has_field = true;
            let prop = Property::new(attrs, field, index, pod, iface, errors);
            if let Some(prop) = prop {
                let name = prop.name.to_string();
                if prop_names.contains(&name) {
                    util::push_error(
                        errors,
                        prop.name.span(),
                        format!("Duplicate definition for property `{}`", name),
                    );
                }
                prop_names.insert(name);
                has_field = prop.storage.has_field();
                properties.push(prop);
            }
            if has_field {
                fs.push(field.clone());
            }
        }

        match &mut fields {
            syn::Fields::Named(f) => f.named = FromIterator::from_iter(fs),
            syn::Fields::Unnamed(f) => f.unnamed = FromIterator::from_iter(fs),
            _ => {}
        }

        Self {
            final_,
            properties,
            fields,
        }
    }
}

pub struct Property {
    field: syn::Field,
    name: PropertyName,
    special_type: PropertyType,
    storage: PropertyStorage,
    override_: Option<PropertyOverride>,
    get: PropertyPermission,
    set: PropertyPermission,
    borrow: bool,
    notify: bool,
    connect_notify: bool,
    nick: Option<String>,
    blurb: Option<String>,
    buildable_props: Vec<(syn::Ident, syn::Lit)>,
    flags: PropertyFlags,
}

impl Property {
    fn new(
        mut attrs: PropertyAttrs,
        field: &syn::Field,
        index: usize,
        pod: bool,
        iface: bool,
        errors: &mut Vec<darling::Error>,
    ) -> Option<Self> {
        attrs.normalize(index, pod);
        attrs.validate(field, pod, iface, errors);
        if attrs.skip.is_some() {
            return None;
        }

        Some(Self {
            field: field.clone(),
            name: attrs.name(index),
            special_type: attrs.special_type(),
            storage: attrs.storage(index, iface),
            override_: attrs.override_(),
            get: (*attrs.get).take().unwrap_or_default(),
            set: (*attrs.set).take().unwrap_or_default(),
            borrow: attrs.borrow.is_some(),
            notify: attrs.notify.unwrap_or(true),
            connect_notify: attrs.connect_notify.unwrap_or(true),
            nick: attrs.nick.take().map(|n| n.value()),
            blurb: attrs.blurb.take().map(|b| b.value()),
            buildable_props: std::mem::take(&mut *attrs.builder).into_iter().collect(),
            flags: attrs.flags(pod),
        })
    }
    pub(crate) fn definition(&self, go: &syn::Ident) -> TokenStream {
        let glib = quote! { #go::glib };
        let name = self.name.to_string();
        if let Some(override_) = &self.override_ {
            return override_.pspec(&name, &glib);
        }
        let nick = self.nick.clone().unwrap_or_else(|| name.clone());
        let blurb = self.blurb.clone().unwrap_or_else(|| name.clone());
        let flags = self.flags.tokens(&glib);
        let ty = self.inner_type(go);
        let props = self
            .buildable_props
            .iter()
            .map(|(ident, value)| quote! { .#ident(#value) });
        let builder = self.special_type.builder(&name, &self.field.ty, go);
        quote_spanned! { self.span() =>
            #builder
            #(#props)*
            .flags(#flags)
            .build()
        }
    }
    fn inner_type(&self, go: &syn::Ident) -> TokenStream {
        let ty = &self.field.ty;
        if self.is_abstract() || matches!(self.storage, PropertyStorage::Computed) {
            quote! { #ty }
        } else {
            quote! { <#ty as #go::ParamStore>::Type }
        }
    }
    fn field_storage(&self, object_type: Option<&TokenStream>, go: &syn::Ident) -> TokenStream {
        let recv = if let Some(object_type) = object_type {
            quote! {
                #go::glib::subclass::prelude::ObjectSubclassIsExt::imp(
                    #go::glib::Cast::upcast_ref::<#object_type>(self)
                )
            }
        } else {
            quote! { self }
        };
        match &self.storage {
            PropertyStorage::NamedField(field) => quote! { #recv.#field },
            PropertyStorage::UnnamedField(index) => quote! { #recv.#index },
            PropertyStorage::Delegate(delegate) => quote! { #recv.#delegate },
            _ => unreachable!("cannot get storage for interface/computed property"),
        }
    }
    fn is_inherited(&self) -> bool {
        self.override_.is_some()
    }
    fn is_abstract(&self) -> bool {
        matches!(
            self.storage,
            PropertyStorage::Abstract | PropertyStorage::InterfaceAbstract
        )
    }
    #[inline]
    fn getter_name(&self) -> syn::Ident {
        format_ident!("{}", self.name.to_string().to_snake_case())
    }
    pub(crate) fn get_impl(&self, index: usize, go: &syn::Ident) -> Option<TokenStream> {
        (self.get.is_allowed() && !self.is_abstract()).then(|| {
            let glib = quote! { #go::glib };
            let body = if let PropertyPermission::AllowCustom(method) = &self.get {
                quote! { #glib::ToValue::to_value(&#method(&obj)) }
            } else {
                let field = self.field_storage(None, go);
                quote! { #go::ParamStoreReadValue::get_value(&#field) }
            };
            quote_spanned! { self.span() =>
                if pspec == &properties[#index] {
                    return #body;
                }
            }
        })
    }
    pub(crate) fn getter_prototype(&self, go: &syn::Ident) -> Option<TokenStream> {
        (!self.is_inherited() && matches!(self.get, PropertyPermission::Allow)).then(|| {
            let method_name = self.getter_name();
            let ty = self.inner_type(go);
            quote_spanned! { self.span() => fn #method_name(&self) -> #ty }
        })
    }
    pub(crate) fn getter_definition(
        &self,
        object_type: &TokenStream,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        self.getter_prototype(go).map(|proto| {
            let body = if self.is_abstract() {
                let name = self.name.to_string();
                quote! { <Self as #go::glib::object::ObjectExt>::property(self, #name) }
            } else {
                let field = self.field_storage(Some(object_type), go);
                quote! { #go::ParamStoreRead::get_owned(&#field) }
            };
            quote_spanned! { self.span() =>
                #proto {
                    #![inline]
                    #body
                }
            }
        })
    }
    #[inline]
    fn borrow_name(&self) -> syn::Ident {
        format_ident!("borrow_{}", self.name.to_string().to_snake_case())
    }
    pub(crate) fn borrow_prototype(&self, go: &syn::Ident) -> Option<TokenStream> {
        self.borrow.then(|| {
            let method_name = self.borrow_name();
            let ty = if self.is_abstract() {
                self.inner_type(go)
            } else {
                let ty = &self.field.ty;
                quote! { <#ty as #go::ParamStoreBorrow<'_>>::BorrowType }
            };
            quote_spanned! { self.span() => fn #method_name(&self) -> #ty }
        })
    }
    pub(crate) fn borrow_definition(
        &self,
        object_type: &TokenStream,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        self.borrow_prototype(go).map(|proto| {
            let field = self.field_storage(Some(object_type), go);
            quote_spanned! { self.span() =>
                #proto {
                    #go::ParamStoreBorrow::borrow(&#field)
                }
            }
        })
    }
    #[inline]
    fn is_set_inline(&self) -> bool {
        self.flags
            .contains(PropertyFlags::EXPLICIT_NOTIFY | PropertyFlags::LAX_VALIDATION)
    }
    #[inline]
    fn setter_name(&self) -> syn::Ident {
        format_ident!("set_{}", self.name.to_string().to_snake_case())
    }
    #[inline]
    fn inline_set_impl<N>(
        &self,
        object_type: Option<&TokenStream>,
        notify: N,
        go: &syn::Ident,
    ) -> TokenStream
    where
        N: FnOnce() -> TokenStream,
    {
        let field = self.field_storage(object_type, go);
        let construct_only = self.flags.contains(PropertyFlags::CONSTRUCT_ONLY);
        if self.get.is_allowed() && !construct_only {
            let notify = notify();
            quote! {
                if #go::ParamStoreWriteChanged::set_owned_checked(&#field, value) {
                    #notify
                }
            }
        } else {
            quote! {
                #go::ParamStoreWrite::set_owned(&#field, value);
            }
        }
    }
    pub(crate) fn set_impl(&self, index: usize, go: &syn::Ident) -> Option<TokenStream> {
        (self.set.is_allowed() && !self.is_abstract()).then(|| {
            let glib = quote! { #go::glib };
            let body = if let PropertyPermission::AllowCustom(method) = &self.set {
                let ty = self.inner_type(go);
                quote! {
                    #method(&obj, value.get::<#ty>().unwrap());
                }
            } else if self.is_set_inline() {
                let body = self.inline_set_impl(
                    None,
                    || quote! {
                        <<Self as #glib::subclass::types::ObjectSubclass>::Type as #glib::object::ObjectExt>::notify_by_pspec(
                            obj,
                            pspec
                        );
                    },
                    go
                );
                let ty = self.inner_type(go);
                quote! {
                    let value = value.get::<#ty>().unwrap();
                    #body
                }
            } else {
                let field = self.field_storage(None, go);
                quote! {
                    #go::ParamStoreWrite::set_value(&#field, &value);
                }
            };
            quote_spanned! { self.span() =>
                if pspec == &properties[#index] {
                    #body
                    return;
                }
            }
        })
    }
    pub(crate) fn setter_prototype(&self, go: &syn::Ident) -> Option<TokenStream> {
        let construct_only = self.flags.contains(PropertyFlags::CONSTRUCT_ONLY);
        let allowed = match &self.set {
            PropertyPermission::Allow => true,
            PropertyPermission::AllowCustom(_) => !self.is_set_inline(),
            _ => false,
        };
        (allowed && !construct_only && !self.is_inherited()).then(|| {
            let method_name = self.setter_name();
            let ty = self.inner_type(go);
            quote_spanned! { self.span() => fn #method_name(&self, value: #ty) }
        })
    }
    pub(crate) fn setter_definition(
        &self,
        index: usize,
        object_type: &TokenStream,
        properties_path: &TokenStream,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        self.setter_prototype(go).map(|proto| {
            let body = if !self.is_abstract() && self.is_set_inline() {
                self.inline_set_impl(
                    Some(object_type),
                    || {
                        quote! {
                            <Self as #go::glib::object::ObjectExt>::notify_by_pspec(
                                self,
                                &#properties_path()[#index]
                            );
                        }
                    },
                    go,
                )
            } else {
                let name = self.name.to_string();
                quote! {
                    <Self as #go::glib::object::ObjectExt>::set_property(self, #name, value);
                }
            };
            quote_spanned! { self.span() =>
                #proto {
                    #![inline]
                    #body
                }
            }
        })
    }
    pub(crate) fn pspec_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        let method_name = format_ident!("pspec_{}", self.name.to_string().to_snake_case());
        Some(quote_spanned! { self.span() => fn #method_name() -> &'static #glib::ParamSpec })
    }
    pub(crate) fn pspec_definition(
        &self,
        index: usize,
        properties_path: &TokenStream,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        self.pspec_prototype(glib).map(|proto| {
            quote_spanned! { self.span() =>
                #proto {
                    #![inline]
                    &#properties_path()[#index]
                }
            }
        })
    }
    pub(crate) fn notify_prototype(&self) -> Option<TokenStream> {
        (!self.is_inherited()
            && self.get.is_allowed()
            && !self.flags.contains(PropertyFlags::CONSTRUCT_ONLY)
            && self.notify)
            .then(|| {
                let method_name = format_ident!("notify_{}", self.name.to_string().to_snake_case());
                quote_spanned! { self.span() => fn #method_name(&self) }
            })
    }
    pub(crate) fn notify_definition(
        &self,
        index: usize,
        properties_path: &TokenStream,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        self.notify_prototype().map(|proto| {
            quote_spanned! { self.span() =>
                #proto {
                    #![inline]
                    <Self as #glib::object::ObjectExt>::notify_by_pspec(
                        self,
                        &#properties_path[#index]
                    );
                }
            }
        })
    }
    pub(crate) fn connect_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        (!self.is_inherited()
            && self.get.is_allowed()
            && !self.flags.contains(PropertyFlags::CONSTRUCT_ONLY)
            && self.connect_notify)
            .then(|| {
                let method_name =
                    format_ident!("connect_{}_notify", self.name.to_string().to_snake_case());
                quote_spanned! { self.span() =>
                    fn #method_name<F: Fn(&Self) + 'static>(&self, f: F) -> #glib::SignalHandlerId
                }
            })
    }
    pub(crate) fn connect_definition(&self, glib: &TokenStream) -> Option<TokenStream> {
        self.connect_prototype(glib).map(|proto| {
            let name = self.name.to_string();
            quote_spanned! { self.span() =>
                #proto {
                    #![inline]
                    <Self as #glib::object::ObjectExt>::connect_notify_local(
                        self,
                        Some(#name),
                        move |recv, _| f(recv),
                    )
                }
            }
        })
    }
}

impl Spanned for Property {
    fn span(&self) -> Span {
        self.field.span()
    }
}
