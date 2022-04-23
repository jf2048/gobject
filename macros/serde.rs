use darling::{
    util::{Flag, PathList, SpannedValue},
    FromAttributes, FromMeta,
};
use gobject_core::{
    util, PropertyOverride, PropertyPermission, PropertyStorage, TypeBase, TypeDefinition,
};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::{parse_quote, spanned::Spanned};

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(gobject_serde))]
struct SerdeAttrs {
    pub serialize: Flag,
    pub deserialize: Flag,
    pub skip_parent: SpannedValue<Flag>,
    pub child_types: PathList,
}

pub(crate) fn extend_serde(
    def: &mut TypeDefinition,
    final_: bool,
    abstract_: bool,
    parent_type: Option<&syn::Path>,
    ext_trait: Option<&syn::Ident>,
    ns: Option<&syn::Ident>,
    errors: &util::Errors,
) {
    let attr = def
        .properties_item_mut()
        .and_then(|item| util::extract_attrs(&mut item.attrs, "gobject_serde"))
        .map(|attrs| {
            let span = attrs
                .iter()
                .map(|a| a.span())
                .reduce(|a, b| a.join(b).unwrap_or(a))
                .unwrap_or_else(Span::call_site);
            let attrs = util::parse_attributes::<SerdeAttrs>(&attrs, errors);
            if attrs.serialize.is_none() && attrs.deserialize.is_none() {
                errors.push(
                    span,
                    "Must have at least one of these attributes: `serialize`, `deserialize`",
                );
            }
            attrs
        })
        .unwrap_or_default();
    let ser = attr.serialize.is_some();
    let de = attr.deserialize.is_some();
    let skip_parent = (*attr.skip_parent).is_some();
    let child_types = attr.child_types;

    if !ser && !de {
        return;
    }

    let mut struct_attrs = Vec::new();
    let storages = def
        .properties
        .iter()
        .map(|p| p.storage.clone())
        .collect::<Vec<_>>();
    if let Some(item) = def.properties_item_mut() {
        if let Some(attrs) = util::extract_attrs(&mut item.attrs, "serde") {
            struct_attrs.extend(attrs);
        }
        for storage in storages {
            let field = match &storage {
                PropertyStorage::NamedField(ident) => item
                    .fields
                    .iter_mut()
                    .find(|f| f.ident.as_ref() == Some(ident)),
                PropertyStorage::UnnamedField(id) => item.fields.iter_mut().nth(*id),
                _ => None,
            };
            if let Some(f) = field {
                util::extract_attrs(&mut f.attrs, "serde");
            }
        }
    }

    let go = &def.crate_ident;
    let sub_ty = match &def.name {
        Some(name) => name,
        None => return,
    };
    let wrapper_ty = parse_quote! { super::#sub_ty };

    if !struct_attrs.iter().any(|a| has_meta(a, "crate")) {
        let crate_ = (quote! { #go::serde }).to_string();
        struct_attrs.push(syn::parse_quote! { #[serde(crate = #crate_)] });
    }
    if !struct_attrs.iter().any(|a| has_meta(a, "rename")) {
        let gtype_name = if let Some(ns) = ns {
            format!("{}{}", ns, sub_ty)
        } else {
            sub_ty.to_string()
        };
        struct_attrs.push(syn::parse_quote! { #[serde(rename = #gtype_name)] });
    }

    let props = def
        .properties
        .iter()
        .filter(|prop| {
            !matches!(&prop.override_, Some(PropertyOverride::Class(_)))
                && prop
                    .field
                    .attrs
                    .iter()
                    .all(|a| !(a.path.is_ident("serde") && has_meta(a, "skip")))
        })
        .collect::<Vec<_>>();

    let mut parent_name = String::from("parent");
    while props.iter().any(|p| {
        let mut is_named = false;
        let name_taken = p
            .field
            .attrs
            .iter()
            .any(|a| has_name(a, &parent_name, &mut is_named));
        name_taken || (!is_named && p.getter_name() == parent_name)
    }) {
        parent_name.insert(0, '_');
    }
    let parent_name = syn::Ident::new(&parent_name, Span::mixed_site());

    let struct_vis = (!final_).then(|| quote! { pub });

    let ser = ser.then(|| {
        let writer = format_ident!("____{}Writer", sub_ty, span = Span::mixed_site());
        let mut getters = Vec::new();
        let write_fields = props.iter().filter_map(|prop| {
            if !prop.get.is_allowed() {
                return None;
            }
            let mut attrs = prop
                .field
                .attrs
                .iter()
                .filter(|a| a.path.is_ident("serde"))
                .cloned()
                .collect::<Vec<_>>();
            let name = prop.getter_name();
            let inner_ty = prop.inner_type(&def.crate_ident);
            if !attrs.iter().any(|a| has_meta(a, "getter")) {
                let getter = match &prop.get {
                    PropertyPermission::Allow | PropertyPermission::AllowCustomDefault => {
                        let path = ext_trait
                            .map(|t| quote! { #t })
                            .unwrap_or_else(|| quote! { #wrapper_ty });
                        Some(quote! { #path::#name })
                    }
                    PropertyPermission::AllowNoMethod => {
                        let pname = prop.name.to_string();
                        getters.push(quote! {
                            fn #name(obj: &#wrapper_ty) -> #inner_ty {
                                #go::glib::prelude::ObjectExt::property(obj, #pname)
                            }
                        });
                        Some(quote! { self::____getters::#name })
                    }
                    PropertyPermission::AllowCustom(p) => Some(quote! { #p }),
                    _ => unreachable!(),
                };
                if let Some(getter) = getter {
                    let getter = getter.to_string();
                    attrs.push(syn::parse_quote! { #[serde(getter = #getter)] });
                }
            }
            Some(quote! { #(#attrs)* #name: #inner_ty })
        }).collect::<Vec<_>>();
        let getters = (!getters.is_empty()).then(|| quote! {
            #[doc(hidden)]
            mod ____getters {
                #(pub(super) #getters)*
            }
        });

        let parent_field = (!skip_parent).then(|| parent_type).flatten().map(|ty| {
            let getter = (quote! {#go::glib::Cast::upcast_ref}).to_string();
            let with = (quote! {
                <#ty as #go::SerializeParent>::SerializeParentType
            }).to_string();
            quote! {
                #[serde(getter = #getter)]
                #[serde(with = #with)]
                #parent_name: <#sub_ty as #go::glib::subclass::types::ObjectSubclass>::ParentType,
            }
        });
        let struct_head = def.generics.as_ref().map(|generics| {
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            quote! { struct #writer #impl_generics #where_clause }
        }).unwrap_or_else(|| quote! { struct #writer });
        let remote = def.generics.as_ref().map(|generics| {
            let (_, type_generics, _) = generics.split_for_impl();
            quote! { super::#sub_ty #type_generics }
        }).unwrap_or_else(|| quote! { super::#sub_ty }).to_string();
        let writer_struct = quote! {
            #[derive(#go::serde::Serialize)]
            #[serde(remote = #remote)]
            #(#struct_attrs)*
            #struct_vis #struct_head {
                #parent_field
                #(#write_fields),*
            }
        };
        if final_ {
            let ser_head = def.trait_head(&wrapper_ty, quote! { #go::serde::Serialize });
            quote! {
                #getters
                #ser_head {
                    fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                    where
                        S: #go::serde::Serializer
                    {
                        #writer_struct
                        #writer::serialize(self, serializer)
                    }
                }
            }
        } else {
            let ser = if !child_types.is_empty() {
                let ser_head = def.trait_head(&wrapper_ty, quote! { #go::serde::Serialize });
                let fallback_writer = (def.base == TypeBase::Class && !abstract_).then(|| {
                    let writer = syn::Ident::new("____Writer", Span::mixed_site());
                    let mut generics = def.generics.clone().unwrap_or_default();
                    generics.params.push(parse_quote! { '____writer });
                    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
                    let def = quote! {
                        struct #writer #type_generics #where_clause(&'____writer #wrapper_ty);

                        impl #impl_generics #go::serde::Serialize for #writer #type_generics #where_clause {
                            fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                            where
                                S: #go::serde::Serializer
                            {
                                <#wrapper_ty as #go::SerializeParent>::SerializeParentType::serialize(self.0, serializer)
                            }
                        }
                    };
                    (writer, def)
                });
                let casts = serialize_child_types(
                    &*child_types,
                    &wrapper_ty,
                    fallback_writer,
                    go,
                );
                Some(quote! {
                    #ser_head {
                        fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                        where
                            S: #go::serde::Serializer
                        {
                            let obj = self;
                            #casts
                        }
                    }
                })
            } else if !abstract_ {
                let ser_head = def.trait_head(&wrapper_ty, quote! { #go::serde::Serialize });
                Some(quote! {
                    #ser_head {
                        fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                        where
                            S: #go::serde::Serializer
                        {
                            #writer::serialize(self, serializer)
                        }
                    }
                })
            } else {
                None
            };
            let writer_def = (def.base == TypeBase::Class).then(|| {
                let parent_head = def.trait_head(&wrapper_ty, quote! { #go::SerializeParent });
                quote! {
                    #getters
                    #[doc(hidden)]
                    #writer_struct
                    #parent_head {
                        type SerializeParentType = #writer;
                    }
                }
            });
            quote! {
                #ser
                #writer_def
            }
        }
    });

    let de = de.then(|| {
        let reader = format_ident!("____{}Reader", sub_ty, span = Span::mixed_site());
        let struct_head = def.generics.as_ref().map(|generics| {
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            quote! { struct #reader #impl_generics #where_clause }
        }).unwrap_or_else(|| quote! { struct #reader });
        let parent_ty = (!skip_parent).then(|| parent_type).flatten();
        let parent_field = parent_ty.map(|ty| quote! {
            #parent_name: <#ty as #go::DeserializeParent>::DeserializeParentType,
        });
        let read_fields = props.iter().filter_map(|prop| {
            if !prop.set.is_allowed() {
                return None;
            }
            let attrs = prop
                .field
                .attrs
                .iter()
                .filter(|a| a.path.is_ident("serde"));
            let name = prop.getter_name();
            let ty = prop.inner_type(&def.crate_ident);
            Some(quote! { #(#attrs)* #name: #ty })
        });
        let reader_struct = quote! {
            #[derive(#go::serde::Deserialize)]
            #(#struct_attrs)*
            #struct_vis #struct_head {
                #parent_field
                #(#read_fields),*
            }
        };

        let reader_path = def.generics.as_ref().map(|generics| {
            let (_, type_generics, _) = generics.split_for_impl();
            quote! { #reader #type_generics }
        }).unwrap_or_else(|| quote! { #reader });
        let try_from_head = def.trait_head(
            &parse_quote! { #wrapper_ty },
            quote! { ::std::convert::TryFrom<#reader_path> },
        );
        let construct_args = props.iter().filter_map(|prop| {
            if !prop.set.is_allowed() {
                return None;
            }
            let name = prop.name.to_string();
            let field = prop.getter_name();
            Some(quote! { (#name, #go::glib::ToValue::to_value(&r.#field)) })
        }).collect::<Vec<_>>();
        let push_current = final_.then(|| {
            let push_parent = parent_ty.map(|_| quote! {
                #go::ParentReader::push_values(
                    &r.#parent_name,
                    &mut args
                );
            });
            quote! {
                #push_parent
                ::std::iter::Extend::extend(
                    &mut args,
                    [#(#construct_args),*]
                );
            }
        }).unwrap_or_else(|| quote! {
            #go::ParentReader::push_values(&r, &mut args);
        });
        let try_from_impl = quote! {
            #try_from_head {
                type Error = #go::glib::BoolError;
                fn try_from(r: #reader_path) -> ::std::result::Result<Self, Self::Error> {
                    let mut args = ::std::vec::Vec::new();
                    #push_current
                    let obj = #go::glib::Object::with_values(
                        <Self as #go::glib::StaticType>::static_type(),
                        &args,
                    )?;
                    #go::glib::Cast::downcast(obj)
                        .map_err(|_| #go::glib::bool_error!("Failed to downcast object"))
                }
            }
        };

        let de_head = def.trait_head_with_params(
            &wrapper_ty,
            quote! { #go::serde::Deserialize<'de> },
            Some([parse_quote! { 'de }]),
        );

        if final_ {
            quote! {
                #de_head {
                    fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                    where
                        D: #go::serde::Deserializer<'de>
                    {
                        #reader_struct
                        #try_from_impl
                        let r = #reader_path::deserialize(deserializer)?;
                        ::std::convert::TryFrom::try_from(r).map_err(#go::serde::de::Error::custom)
                    }
                }
            }
        } else {
            let de = if !child_types.is_empty() {
                let casts = deserialize_child_types(
                    &*child_types,
                    &wrapper_ty,
                    def.base == TypeBase::Class && !abstract_,
                    go,
                );
                Some(quote! {
                    #de_head {
                        fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                        where
                            D: #go::serde::Deserializer<'de>
                        {
                            #casts
                        }
                    }
                })
            } else if !abstract_ {
                Some(quote! {
                    #de_head {
                        fn deserialize<D>(deserializer: D) -> ::std::result::Result<Self, D::Error>
                        where
                            D: #go::serde::Deserializer<'de>
                        {
                            let r = #reader_path::deserialize(deserializer)?;
                            ::std::convert::TryFrom::try_from(r).map_err(#go::serde::de::Error::custom)
                        }
                    }
                })
            } else {
                None
            };
            let reader_def = (def.base == TypeBase::Class).then(|| {
                let parent_head = def.trait_head(
                    &wrapper_ty,
                    quote! { #go::DeserializeParent },
                );
                let parent_reader_head = def.trait_head(
                    &parse_quote! { #reader },
                    quote! { #go::ParentReader },
                );
                let push_parent = parent_ty.map(|_| quote! {
                    #go::ParentReader::push_values(
                        &r.#parent_name,
                        values,
                    );
                });
                quote! {
                    #[doc(hidden)]
                    #reader_struct
                    #try_from_impl
                    #parent_head {
                        type DeserializeParentType = #reader;
                    }
                    #parent_reader_head {
                        #[inline]
                        fn push_values(&self, values: &mut ::std::vec::Vec<(&'static ::std::primitive::str, #go::glib::Value)>) {
                            let r = self;
                            #push_parent
                            ::std::iter::Extend::extend(
                                values,
                                [#(#construct_args),*]
                            );
                        }
                    }
                }
            });
            quote! {
                #de
                #reader_def
            }
        }
    });

    let (_, items) = def.module.content.get_or_insert_with(Default::default);
    items.push(syn::Item::Verbatim(quote! { #ser #de }));
}

#[inline]
fn has_meta(attr: &syn::Attribute, meta: &str) -> bool {
    attr.parse_meta()
        .map(|m| match m {
            syn::Meta::List(l) => l.nested.iter().any(|m| match m {
                syn::NestedMeta::Meta(m) => m.path().is_ident(meta),
                _ => false,
            }),
            _ => false,
        })
        .unwrap_or(false)
}

#[inline]
fn has_name(attr: &syn::Attribute, name: &str, is_named: &mut bool) -> bool {
    if !attr.path.is_ident("serde") {
        return false;
    }
    attr.parse_meta()
        .map(|m| match m {
            syn::Meta::List(l) => l.nested.iter().any(|m| match m {
                syn::NestedMeta::Meta(syn::Meta::NameValue(m)) => {
                    if m.path.is_ident("rename") || m.path.is_ident("alias") {
                        if m.path.is_ident("rename") {
                            *is_named = true;
                        }
                        if let syn::Lit::Str(s) = &m.lit {
                            if s.value() == name {
                                return true;
                            }
                        }
                    }
                    false
                }
                _ => false,
            }),
            _ => false,
        })
        .unwrap_or(false)
}

#[inline]
fn serialize_child_types(
    child_types: &[syn::Path],
    wrapper_ty: &syn::Path,
    fallback_writer: Option<(syn::Ident, TokenStream)>,
    go: &syn::Ident,
) -> TokenStream {
    let child_casts = child_types.iter().enumerate().map(|(index, child_ty)| {
        let index = u32::try_from(index).unwrap();
        quote_spanned! { child_ty.span() =>
            if let Some(obj) = #go::glib::Cast::downcast_ref::<#child_ty>(obj) {
                return serializer.serialize_newtype_variant(
                    <#wrapper_ty as #go::glib::StaticType>::static_type().name(),
                    #index,
                    <#child_ty as #go::glib::StaticType>::static_type().name(),
                    obj,
                );
            }
        }
    });
    let fallback = fallback_writer
        .map(|(writer_ident, writer_def)| {
            let index = u32::try_from(child_types.len()).unwrap();
            quote! {
                #writer_def

                serializer.serialize_newtype_variant(
                    <#wrapper_ty as #go::glib::StaticType>::static_type().name(),
                    #index,
                    <#wrapper_ty as #go::glib::StaticType>::static_type().name(),
                    &#writer_ident(obj),
                )
            }
        })
        .unwrap_or_else(|| {
            quote! {
                Err(#go::serde::ser::Error::custom(::std::format!(
                    "Unsupported type `{}`",
                    #go::glib::prelude::ObjectExt::type_(obj).name(),
                )))
            }
        });
    quote! {
        #(#child_casts)*
        #fallback
    }
}

#[inline]
fn deserialize_child_types(
    child_types: &[syn::Path],
    wrapper_ty: &syn::Path,
    fallback: bool,
    go: &syn::Ident,
) -> TokenStream {
    let fallback_ty = fallback.then(|| wrapper_ty);
    let variant_count = child_types.len() + fallback.then(|| 1).unwrap_or(0);
    let child_names = child_types.iter().chain(fallback_ty).map(|cty| {
        quote! {
            <#cty as #go::glib::StaticType>::static_type().name()
        }
    });
    let u64_mappings = child_types
        .iter()
        .chain(fallback_ty)
        .enumerate()
        .map(|(index, cty)| {
            let index = index as u64;
            quote! {
                #index => ::std::result::Result::Ok(<#cty as #go::glib::StaticType>::static_type())
            }
        });
    let u64_error = format!("variant index 0 <= i < {}", variant_count);
    let str_mappings = child_types.iter().chain(fallback_ty).map(|cty| {
        quote! {
            if v == <#cty as glib::StaticType>::static_type().name() {
                return ::std::result::Result::Ok(<#cty as glib::StaticType>::static_type());
            }
        }
    });
    let bytes_mappings = child_types.iter().chain(fallback_ty).map(|cty| {
        quote! {
            if v == <#cty as glib::StaticType>::static_type().name().as_bytes() {
                return ::std::result::Result::Ok(<#cty as glib::StaticType>::static_type());
            }
        }
    });
    let de_mappings = child_types.iter().map(|cty| {
        quote! {
            if ty == <#cty as #go::glib::StaticType>::static_type() {
                let obj = #go::serde::de::VariantAccess::newtype_variant::<#cty>(variant)?;
                return ::std::result::Result::Ok(#go::glib::Cast::upcast(obj));
            }
        }
    });
    let fallback_mapping = fallback.then(|| {
        quote! {
            if ty == <#wrapper_ty as #go::glib::StaticType>::static_type() {
                let r = #go::serde::de::VariantAccess::newtype_variant::<<#wrapper_ty as #go::DeserializeParent>::DeserializeParentType>(variant)?;
                return ::std::convert::TryFrom::try_from(r).map_err(#go::serde::de::Error::custom);
            }
        }
    });
    quote! {
        static VARIANTS: #go::SyncOnceCell<[&'static ::std::primitive::str; #variant_count]>
            = #go::SyncOnceCell::new();
        let variants = VARIANTS.get_or_init(|| [#(#child_names),*]);
        struct ____FieldVisitor;
        impl<'de> #go::serde::de::Visitor<'de> for ____FieldVisitor {
            type Value = #go::glib::Type;
            fn expecting(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                f.write_str("variant identifier")
            }
            fn visit_u64<E: #go::serde::de::Error>(self, v: ::std::primitive::u64) -> ::std::result::Result<Self::Value, E> {
                match v {
                    #(#u64_mappings,)*
                    _ => ::std::result::Result::Err(#go::serde::de::Error::invalid_value(
                        #go::serde::de::Unexpected::Unsigned(v as u64),
                        &#u64_error,
                    ))
                }
            }
            fn visit_str<E: #go::serde::de::Error>(self, v: &::std::primitive::str) -> ::std::result::Result<Self::Value, E> {
                #(#str_mappings)*
                ::std::result::Result::Err(#go::serde::de::Error::unknown_variant(v, VARIANTS.get().unwrap()))
            }
            fn visit_bytes<E: #go::serde::de::Error>(self, v: &[::std::primitive::u8]) -> ::std::result::Result<Self::Value, E> {
                #(#bytes_mappings)*
                ::std::result::Result::Err(#go::serde::de::Error::unknown_variant(
                    &::std::string::String::from_utf8_lossy(v),
                    VARIANTS.get().unwrap()
                ))
            }
        }
        impl<'de> #go::serde::de::DeserializeSeed<'de> for ____FieldVisitor {
            type Value = #go::glib::Type;
            fn deserialize<D>(self, deserializer: D) -> ::std::result::Result<Self::Value, D::Error>
            where
                D: #go::serde::Deserializer<'de>
            {
                deserializer.deserialize_identifier(self)
            }
        }
        struct ____Visitor;
        impl<'de> serde::de::Visitor<'de> for ____Visitor {
            type Value = #wrapper_ty;
            fn expecting(&self, f: &mut ::std::fmt::Formatter) -> ::std::fmt::Result {
                f.write_str(<#wrapper_ty as #go::glib::StaticType>::static_type().name())
            }
            fn visit_enum<A>(self, data: A) -> ::std::result::Result<Self::Value, A::Error>
            where
                A: #go::serde::de::EnumAccess<'de>,
            {
                let (ty, variant) = data.variant_seed(____FieldVisitor)?;
                #(#de_mappings)*
                #fallback_mapping
                ::std::unreachable!()
            }
        }
        deserializer.deserialize_enum(
            <#wrapper_ty as #go::glib::StaticType>::static_type().name(),
            variants.as_slice(),
            ____Visitor,
        )
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct EnumAttrs {
    pub serialize: Flag,
    pub deserialize: Flag,
    pub parent: Option<syn::Path>,
    pub fallback: Flag,
    pub child_types: PathList,
}

pub(crate) fn downcast_enum(
    args: TokenStream,
    go: &syn::Ident,
    errors: &gobject_core::util::Errors,
) -> TokenStream {
    let span = args.span();
    let attrs = gobject_core::util::parse_list::<EnumAttrs>(args, errors);
    let ty = match &attrs.parent {
        Some(ty) => ty,
        None => return Default::default(),
    };
    if attrs.parent.is_none() {
        errors.push(span, "Missing required attribute `parent`");
    }
    if attrs.serialize.is_none() && attrs.deserialize.is_none() {
        errors.push(
            span,
            "Must have at least one of these attributes: `serialize`, `deserialize`",
        );
    }
    let ser = attrs.serialize.is_some().then(|| {
        let fallback_writer = attrs.fallback.map(|_| {
            let writer = syn::Ident::new("____Writer", Span::mixed_site());
            let def = quote! {
                struct #writer<'w>(&'w #ty);

                impl<'w> #go::serde::Serialize for #writer<'w> {
                    fn serialize<S>(&self, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
                    where
                        S: #go::serde::Serializer
                    {
                        <#ty as #go::SerializeParent>::SerializeParentType::serialize(self.0, serializer)
                    }
                }
            };
            (writer, def)
        });
        let casts = serialize_child_types(&*attrs.child_types, ty, fallback_writer, go);
        quote! {
            fn serialize<S>(obj: &#ty, serializer: S) -> ::std::result::Result<S::Ok, S::Error>
            where
                S: #go::serde::Serializer
            {
                #casts
            }
        }
    });
    let de = attrs.deserialize.is_some().then(|| {
        let casts = deserialize_child_types(&*attrs.child_types, ty, attrs.fallback.is_some(), go);
        quote! {
            fn deserialize<D>(deserializer: D) -> ::std::result::Result<#ty, D::Error>
            where
                D: #go::serde::Deserializer<'de>
            {
                #casts
            }
        }
    });
    quote! {
        #ser
        #de
    }
}
