use crate::{
    util::{self, Errors},
    TypeBase,
};
use darling::{
    util::{Flag, SpannedValue},
    FromDeriveInput, FromField, FromMeta, ToTokens,
};
use heck::ToSnakeCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
};
use syn::spanned::Spanned;

#[derive(FromDeriveInput)]
#[darling(default, attributes(properties))]
pub(crate) struct PropertiesAttrs {
    pod: Flag,
    final_type: Option<syn::Ident>,
    interface: SpannedValue<Flag>,
    data: darling::ast::Data<darling::util::Ignored, PropertyAttrs>,
}

impl Default for PropertiesAttrs {
    fn default() -> Self {
        Self {
            pod: Default::default(),
            final_type: None,
            interface: Default::default(),
            data: darling::ast::Data::empty_from(&syn::Data::Struct(syn::DataStruct {
                struct_token: Default::default(),
                fields: syn::Fields::Unit,
                semi_token: Some(Default::default()),
            })),
        }
    }
}

#[derive(Debug, Default, FromField)]
#[darling(default, attributes(property))]
struct PropertyAttrs {
    ident: Option<syn::Ident>,
    skip: SpannedValue<Flag>,
    get: SpannedValue<Option<PropertyPermission>>,
    set: SpannedValue<Option<PropertyPermission>>,
    borrow: SpannedValue<Flag>,
    construct: SpannedValue<Option<bool>>,
    construct_only: SpannedValue<Option<bool>>,
    lax_validation: SpannedValue<Option<bool>>,
    user_1: SpannedValue<Option<bool>>,
    user_2: SpannedValue<Option<bool>>,
    user_3: SpannedValue<Option<bool>>,
    user_4: SpannedValue<Option<bool>>,
    user_5: SpannedValue<Option<bool>>,
    user_6: SpannedValue<Option<bool>>,
    user_7: SpannedValue<Option<bool>>,
    user_8: SpannedValue<Option<bool>>,
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
    builder_defaults: Option<syn::ExprArray>,
    builder: SpannedValue<HashMap<syn::Ident, InnerExpr>>,
}

#[derive(Debug)]
struct InnerExpr(syn::Expr);

impl FromMeta for InnerExpr {
    fn from_list(items: &[syn::NestedMeta]) -> darling::Result<Self> {
        if items.len() != 1 {
            return Err(darling::Error::unsupported_format(
                "nested meta with length other than 1",
            ));
        }
        let meta = items.first().unwrap();
        let lit = match meta {
            syn::NestedMeta::Lit(syn::Lit::Str(lit)) => lit,
            syn::NestedMeta::Lit(lit) => {
                return Err(darling::Error::unexpected_lit_type(lit));
            }
            syn::NestedMeta::Meta(_) => {
                return Err(darling::Error::unsupported_format("meta"));
            }
        };
        Ok(Self(syn::parse_str(&lit.value())?))
    }
    fn from_value(value: &syn::Lit) -> darling::Result<Self> {
        Ok(Self(syn::Expr::Lit(syn::ExprLit {
            attrs: Vec::new(),
            lit: value.clone(),
        })))
    }
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
    fn storage(&self, index: usize, base: TypeBase) -> PropertyStorage {
        if base == TypeBase::Interface {
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
        } else {
            self.override_iface
                .as_ref()
                .map(|path| PropertyOverride::Interface(path.clone()))
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
        flags.set(PropertyFlags::USER_1, self.user_1.unwrap_or(false));
        flags.set(PropertyFlags::USER_2, self.user_2.unwrap_or(false));
        flags.set(PropertyFlags::USER_3, self.user_3.unwrap_or(false));
        flags.set(PropertyFlags::USER_4, self.user_4.unwrap_or(false));
        flags.set(PropertyFlags::USER_5, self.user_5.unwrap_or(false));
        flags.set(PropertyFlags::USER_6, self.user_6.unwrap_or(false));
        flags.set(PropertyFlags::USER_7, self.user_7.unwrap_or(false));
        flags.set(PropertyFlags::USER_8, self.user_8.unwrap_or(false));
        flags.set(
            PropertyFlags::EXPLICIT_NOTIFY,
            self.explicit_notify.unwrap_or(false),
        );
        flags.set(PropertyFlags::DEPRECATED, self.deprecated.unwrap_or(false));
        flags
    }
    fn normalize(&mut self, field: &syn::Field, pod: bool) {
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
        } else if !field.attrs.iter().any(|a| a.path.is_ident("property")) {
            self.skip = SpannedValue::new(Flag::present(), Span::call_site());
        }
        let computed = self.computed.is_some();
        if let Some(get) = self.get.as_mut() {
            get.normalize(computed);
        }
        if let Some(set) = self.set.as_mut() {
            set.normalize(computed);
        }
    }
    fn validate(&self, field: &syn::Field, pod: bool, base: TypeBase, errors: &Errors) {
        use crate::validations::*;

        if self.skip.is_none() && self.ident.is_none() && self.name.is_none() {
            errors.push_spanned(
                field,
                "#[property(name = \"...\")] required for tuple struct properties",
            );
        }

        let name = self.name(0);
        if !util::is_valid_name(&name.to_string()) {
            errors.push(
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
            errors.push_spanned(field, "Property must be readable or writable");
        }

        let interface = (
            "interface",
            (base == TypeBase::Interface).then(|| field.span()),
        );
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
        let user_1 = ("user_1", check_bool(&self.user_1));
        let user_2 = ("user_2", check_bool(&self.user_2));
        let user_3 = ("user_3", check_bool(&self.user_3));
        let user_4 = ("user_4", check_bool(&self.user_4));
        let user_5 = ("user_5", check_bool(&self.user_5));
        let user_6 = ("user_6", check_bool(&self.user_6));
        let user_7 = ("user_7", check_bool(&self.user_7));
        let user_8 = ("user_8", check_bool(&self.user_8));
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
                    &user_1,
                    &user_2,
                    &user_3,
                    &user_4,
                    &user_5,
                    &user_6,
                    &user_7,
                    &user_8,
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
                    errors.push(
                        self.borrow.span(),
                        format!("`borrow` not allowed on {} property", attr_name),
                    );
                }
            }
        }
    }
}

#[derive(Debug)]
struct PropertyStorageAttr(syn::Expr);

impl FromMeta for PropertyStorageAttr {
    fn from_string(value: &str) -> darling::Result<Self> {
        Ok(Self(syn::parse_str(value)?))
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum PropertyPermission {
    Deny,
    Allow,
    AllowNoMethod,
    AllowCustomDefault,
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
        if value == "_" {
            return Ok(Self::AllowCustomDefault);
        }
        Ok(Self::AllowCustom(syn::parse_str(value)?))
    }
}

impl PropertyPermission {
    fn normalize(&mut self, computed: bool) {
        if computed && matches!(self, Self::Allow) {
            *self = Self::AllowCustomDefault;
        }
    }
    pub fn is_allowed(&self) -> bool {
        !matches!(self, Self::Deny)
    }
}

bitflags::bitflags! {
    pub struct PropertyFlags: u32 {
        const READABLE        = 1 << 0;
        const WRITABLE        = 1 << 1;
        const CONSTRUCT       = 1 << 2;
        const CONSTRUCT_ONLY  = 1 << 3;
        const LAX_VALIDATION  = 1 << 4;
        const USER_1          = 256;
        const USER_2          = 1024;
        const USER_3          = 2048;
        const USER_4          = 4096;
        const USER_5          = 8192;
        const USER_6          = 16384;
        const USER_7          = 32768;
        const USER_8          = 65536;
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

#[derive(Debug, Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub enum PropertyType {
    Unspecified,
    Enum,
    Flags,
    Boxed,
    Object,
}

impl PropertyType {
    fn builder(
        &self,
        name: &str,
        extra: &[syn::Expr],
        ty: &TokenStream,
        go: &syn::Ident,
    ) -> TokenStream {
        let glib = quote! { #go::glib };
        let pspec_type = match self {
            Self::Unspecified => {
                return quote! {
                    <#ty as #go::ParamSpecBuildable>::ParamSpec::builder(#name, #(#extra),*)
                }
            }
            Self::Enum => format_ident!("ParamSpecEnum"),
            Self::Flags => format_ident!("ParamSpecFlags"),
            Self::Boxed => format_ident!("ParamSpecBoxed"),
            Self::Object => format_ident!("ParamSpecObject"),
        };
        quote! {
            #glib::#pspec_type::builder(
                #name,
                <<#ty as #glib::value::ValueType>::Type as #glib::StaticType>::static_type(),
                #(#extra),*
            )
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum PropertyStorage {
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

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum PropertyName {
    Field(syn::Ident),
    Custom(syn::LitStr),
}

impl PropertyName {
    pub fn field_name(&self) -> syn::Ident {
        format_ident!("{}", self.to_string().to_snake_case())
    }
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
            PropertyName::Field(name) => util::format_name(name).fmt(f),
            PropertyName::Custom(name) => name.value().fmt(f),
        }
    }
}

#[derive(Debug, Eq, PartialEq, Clone)]
pub enum PropertyOverride {
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
    pub(crate) final_type: Option<syn::Ident>,
    pub(crate) base: TypeBase,
    pub(crate) properties: Vec<Property>,
    pub(crate) fields: syn::Fields,
}

impl Default for Properties {
    fn default() -> Self {
        Self {
            final_type: None,
            base: TypeBase::Class,
            properties: Vec::new(),
            fields: syn::Fields::Unit,
        }
    }
}

impl Properties {
    pub(crate) fn from_derive_input(
        input: &syn::DeriveInput,
        base: Option<TypeBase>,
        errors: &Errors,
    ) -> Self {
        let PropertiesAttrs {
            pod,
            final_type,
            interface,
            data,
        } = match PropertiesAttrs::from_derive_input(input) {
            Ok(attrs) => attrs,
            Err(e) => {
                errors.push_darling(e);
                Default::default()
            }
        };
        if base.is_none() {
            if let Some(final_type) = &final_type {
                errors.push_spanned(final_type, "`final_type` not allowed here");
            }
        } else if interface.is_some() {
            errors.push(interface.span(), "`interface` not allowed here");
        }
        let pod = pod.is_some();
        let base = base.unwrap_or_else(|| {
            interface
                .map(|_| TypeBase::Interface)
                .unwrap_or(TypeBase::Class)
        });
        let data = data.take_struct().map(|s| s.fields).unwrap_or_default();

        let fields = match &input.data {
            syn::Data::Struct(syn::DataStruct { fields, .. }) => fields,
            _ => return Default::default(),
        };

        let mut prop_names = HashSet::new();
        let mut properties = vec![];
        let mut out_fields = Vec::new();
        for (index, (attrs, mut field)) in
            std::iter::zip(data, fields.clone().into_iter()).enumerate()
        {
            let prop = Property::new(attrs, &field, index, pod, base, errors);
            let mut has_field = true;
            if let Some(prop) = prop {
                let name = prop.name.to_string();
                if prop_names.contains(&name) {
                    errors.push(
                        prop.name.span(),
                        format!("Duplicate definition for property `{}`", name),
                    );
                }
                prop_names.insert(name);
                has_field = prop.storage.has_field();
                properties.push(prop);
            }
            while let Some(index) = field.attrs.iter().position(|a| a.path.is_ident("property")) {
                field.attrs.remove(index);
            }
            if has_field {
                out_fields.push(field);
            }
        }

        let fields = match fields {
            syn::Fields::Named(_) => syn::Fields::Named(syn::FieldsNamed {
                brace_token: Default::default(),
                named: FromIterator::from_iter(out_fields),
            }),
            syn::Fields::Unnamed(_) => syn::Fields::Unnamed(syn::FieldsUnnamed {
                paren_token: Default::default(),
                unnamed: FromIterator::from_iter(out_fields),
            }),
            f => f.clone(),
        };

        Self {
            final_type,
            base,
            properties,
            fields,
        }
    }
}

#[derive(Debug)]
pub struct Property {
    pub field: syn::Field,
    pub name: PropertyName,
    pub special_type: PropertyType,
    pub storage: PropertyStorage,
    pub override_: Option<PropertyOverride>,
    pub get: PropertyPermission,
    pub set: PropertyPermission,
    pub borrow: bool,
    pub notify: bool,
    pub connect_notify: bool,
    pub nick: Option<String>,
    pub blurb: Option<String>,
    pub buildable_defaults: Vec<syn::Expr>,
    pub buildable_props: Vec<(syn::Ident, syn::Expr)>,
    pub flags: PropertyFlags,
}

impl Property {
    fn new(
        mut attrs: PropertyAttrs,
        field: &syn::Field,
        index: usize,
        pod: bool,
        base: TypeBase,
        errors: &Errors,
    ) -> Option<Self> {
        attrs.normalize(field, pod);
        attrs.validate(field, pod, base, errors);
        if attrs.skip.is_some() {
            return None;
        }

        let flags = attrs.flags(pod);
        Some(Self {
            field: field.clone(),
            name: attrs.name(index),
            special_type: attrs.special_type(),
            storage: attrs.storage(index, base),
            override_: attrs.override_(),
            get: (*attrs.get).take().unwrap_or_default(),
            set: (*attrs.set).take().unwrap_or_default(),
            borrow: attrs.borrow.is_some(),
            notify: attrs.notify.unwrap_or(true),
            connect_notify: attrs.connect_notify.unwrap_or(true),
            nick: attrs.nick.take().map(|n| n.value()),
            blurb: attrs.blurb.take().map(|b| b.value()),
            buildable_defaults: attrs
                .builder_defaults
                .map(|d| d.elems.into_iter().collect())
                .unwrap_or_default(),
            buildable_props: std::mem::take(&mut *attrs.builder)
                .into_iter()
                .map(|(i, e)| (i, e.0))
                .collect(),
            flags,
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
        let builder = self
            .special_type
            .builder(&name, &self.buildable_defaults, &ty, go);
        quote_spanned! { self.span() =>
            #builder
            #(#props)*
            .nick(#nick)
            .blurb(#blurb)
            .flags(#flags)
            .build()
        }
    }
    pub fn inner_type(&self, go: &syn::Ident) -> TokenStream {
        let ty = &self.field.ty;
        quote! { <#ty as #go::ParamStore>::Type }
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
    fn pspec_cmp(&self, index: usize) -> TokenStream {
        let index = index + 1;
        quote! { id == #index }
    }
    pub fn custom_method_path(&self, set: bool) -> Option<Cow<'_, syn::Ident>> {
        let perm = match set {
            true => &self.set,
            false => &self.get,
        };
        match perm {
            PropertyPermission::AllowCustomDefault => {
                let name = self.name.field_name();
                let method = match set {
                    true => format_ident!("set_{}", name),
                    false => format_ident!("{}", name),
                };
                Some(Cow::Owned(method))
            }
            PropertyPermission::AllowCustom(path) if path.segments.len() == 1 => {
                Some(Cow::Borrowed(&path.segments[0].ident))
            }
            _ => None,
        }
    }
    #[inline]
    fn custom_call(
        &self,
        set_ty: Option<&TokenStream>,
        method: Option<&syn::ImplItemMethod>,
    ) -> Option<TokenStream> {
        let perm = match set_ty.is_some() {
            true => &self.set,
            false => &self.get,
        };
        let set_ty = set_ty.map(|s| quote! { value.get::<#s>().unwrap() });
        let method_args = method.map(|m| m.sig.inputs.len());
        let args = if set_ty.is_none() && method_args == Some(0) {
            quote! {}
        } else if set_ty.is_some() && method_args == Some(1) {
            quote! { #set_ty }
        } else if matches!(self.storage, PropertyStorage::InterfaceAbstract)
            || method.map(|m| m.sig.receiver().is_none()).unwrap_or(false)
        {
            quote! { obj, #set_ty }
        } else {
            quote! { self, #set_ty }
        };
        match perm {
            PropertyPermission::AllowCustomDefault => {
                let name = self.name.field_name();
                let method = match set_ty.is_some() {
                    true => format_ident!("set_{}", name),
                    false => format_ident!("{}", name),
                };
                Some(quote! { Self::#method(#args) })
            }
            PropertyPermission::AllowCustom(path) => Some(quote! {
                #path(#args)
            }),
            _ => None,
        }
    }
    #[inline]
    pub fn getter_name(&self) -> syn::Ident {
        let mut name = self.name.field_name().to_string();
        while syn::parse2::<syn::Ident>(syn::Ident::new(&name, Span::call_site()).to_token_stream())
            .is_err()
        {
            name.push('_');
        }
        format_ident!("{}", name)
    }
    pub(crate) fn get_impl(
        &self,
        index: usize,
        method: Option<&syn::ImplItemMethod>,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        (self.get.is_allowed() && !self.is_abstract()).then(|| {
            let glib = quote! { #go::glib };
            let cmp = self.pspec_cmp(index);
            let body = if let Some(call) = self.custom_call(None, method) {
                quote! { #glib::ToValue::to_value(&#call) }
            } else {
                let field = self.field_storage(None, go);
                quote! { #go::ParamStoreReadValue::get_value(&#field) }
            };
            quote_spanned! { self.span() =>
                if #cmp {
                    return #body;
                }
            }
        })
    }
    fn getter_prototype(&self, go: &syn::Ident) -> Option<TokenStream> {
        (!self.is_inherited() && matches!(self.get, PropertyPermission::Allow)).then(|| {
            let method_name = self.getter_name();
            let ty = self.inner_type(go);
            quote_spanned! { self.span() => fn #method_name(&self) -> #ty }
        })
    }
    fn getter_definition(&self, object_type: &TokenStream, go: &syn::Ident) -> Option<TokenStream> {
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
        format_ident!("borrow_{}", self.name.field_name())
    }
    fn borrow_prototype(&self, go: &syn::Ident) -> Option<TokenStream> {
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
    fn borrow_definition(&self, object_type: &TokenStream, go: &syn::Ident) -> Option<TokenStream> {
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
        self.flags.contains(PropertyFlags::LAX_VALIDATION)
    }
    #[inline]
    fn setter_name(&self) -> syn::Ident {
        format_ident!("set_{}", self.name.field_name())
    }
    #[inline]
    fn inline_set_impl<N>(
        &self,
        object_type: Option<&TokenStream>,
        notify: Option<N>,
        go: &syn::Ident,
    ) -> TokenStream
    where
        N: FnOnce() -> TokenStream,
    {
        let field = self.field_storage(object_type, go);
        let construct_only = self.flags.contains(PropertyFlags::CONSTRUCT_ONLY);
        if self.get.is_allowed() && !construct_only {
            if let Some(notify) = notify {
                let notify = notify();
                return quote! {
                    if #go::ParamStoreWriteChanged::set_owned_checked(&#field, value) {
                        #notify
                    }
                };
            }
        }
        quote! {
            #go::ParamStoreWrite::set_owned(&#field, value);
        }
    }
    pub(crate) fn set_impl(
        &self,
        index: usize,
        method: Option<&syn::ImplItemMethod>,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        (self.set.is_allowed() && !self.is_abstract()).then(|| {
            let glib = quote! { #go::glib };
            let cmp = self.pspec_cmp(index);
            let ty = self.inner_type(go);
            let body = if let Some(call) = self.custom_call(Some(&ty), method) {
                quote! { #call; }
            } else if self.is_set_inline() {
                let body = self.inline_set_impl(
                    None,
                    self.flags.contains(PropertyFlags::EXPLICIT_NOTIFY)
                    .then(|| || quote! {
                        <<Self as #glib::subclass::types::ObjectSubclass>::Type as #glib::object::ObjectExt>::notify_by_pspec(
                            obj,
                            pspec
                        );
                    }),
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
                if #cmp {
                    #body
                    return;
                }
            }
        })
    }
    fn setter_prototype(&self, go: &syn::Ident) -> Option<TokenStream> {
        let construct_only = self.flags.contains(PropertyFlags::CONSTRUCT_ONLY);
        let allowed = match &self.set {
            PropertyPermission::Allow => true,
            PropertyPermission::AllowCustom(_) | PropertyPermission::AllowCustomDefault => {
                !self.is_set_inline()
            }
            _ => false,
        };
        (allowed && !construct_only && !self.is_inherited()).then(|| {
            let method_name = self.setter_name();
            let ty = self.inner_type(go);
            quote_spanned! { self.span() => fn #method_name(&self, value: #ty) }
        })
    }
    fn setter_definition(
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
                    Some(|| {
                        quote! {
                            <Self as #go::glib::object::ObjectExt>::notify_by_pspec(
                                self,
                                &#properties_path()[#index]
                            );
                        }
                    }),
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
    fn notify_prototype(&self) -> Option<TokenStream> {
        (!self.is_inherited()
            && self.get.is_allowed()
            && !self.flags.contains(PropertyFlags::CONSTRUCT_ONLY)
            && self.notify)
            .then(|| {
                let method_name = format_ident!("notify_{}", self.name.field_name());
                quote_spanned! { self.span() => fn #method_name(&self) }
            })
    }
    fn notify_definition(
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
                        &#properties_path()[#index]
                    );
                }
            }
        })
    }
    fn connect_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        (!self.is_inherited()
            && self.get.is_allowed()
            && !self.flags.contains(PropertyFlags::CONSTRUCT_ONLY)
            && self.connect_notify)
            .then(|| {
                let method_name =
                    format_ident!("connect_{}_notify", self.name.field_name());
                quote_spanned! { self.span() =>
                    fn #method_name<____Func: Fn(&Self) + 'static>(&self, f: ____Func) -> #glib::SignalHandlerId
                }
            })
    }
    fn connect_definition(&self, glib: &TokenStream) -> Option<TokenStream> {
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
    pub(crate) fn method_prototypes(&self, go: &syn::Ident) -> Vec<TokenStream> {
        let glib = quote! { #go::glib };
        [
            self.setter_prototype(go),
            self.getter_prototype(go),
            self.borrow_prototype(go),
            self.notify_prototype(),
            self.connect_prototype(&glib),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    pub(crate) fn method_definitions(
        &self,
        index: usize,
        ty: &TokenStream,
        properties_path: &TokenStream,
        go: &syn::Ident,
    ) -> Vec<TokenStream> {
        let glib = quote! { #go::glib };
        [
            self.setter_definition(index, ty, properties_path, go),
            self.getter_definition(ty, go),
            self.borrow_definition(ty, go),
            self.notify_definition(index, properties_path, &glib),
            self.connect_definition(&glib),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}

impl Spanned for Property {
    fn span(&self) -> Span {
        self.field.span()
    }
}
