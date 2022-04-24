use super::{VariantBuilder, VariantBuilderExt};
use gio::prelude::*;
use glib::{IsA, ToVariant, Variant, VariantTy, VariantType};
use std::{borrow::Cow, marker::PhantomData};

pub mod file {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(f: &gio::File) -> Variant {
        f.uri().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gio::File> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        Some(gio::File::for_uri(variant.str()?))
    }
    declare_optional!(gio::File);
}

pub mod icon {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant<I: IsA<gio::Icon>>(i: &I) -> Variant {
        IconExt::to_string(i)
            .unwrap_or_else(|| "".into())
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gio::Icon> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        gio::Icon::for_string(variant.str()?).ok()
    }
    declare_optional!(gio::Icon);
}

pub struct ListModel<O>(PhantomData<O>);

impl<O: IsA<glib::Object>> ListModel<O> {
    pub fn static_variant_type() -> Cow<'static, VariantTy>
    where
        O: glib::StaticVariantType,
    {
        let mut builder = glib::GStringBuilder::new("a");
        builder.append(O::static_variant_type().as_str());
        Cow::Owned(VariantType::from_string(builder.into_string()).unwrap())
    }
    pub fn to_variant<M>(m: &M) -> Variant
    where
        M: IsA<gio::ListModel>,
        O: glib::StaticVariantType + ToVariant,
    {
        let builder = VariantBuilder::new(VariantTy::STRING_ARRAY);
        let count = m.n_items();
        for i in 0..count {
            if let Some(o) = m.item(i).and_then(|o| o.downcast::<O>().ok()) {
                unsafe {
                    builder.add(&o);
                }
            }
        }
        builder.end()
    }
}

pub struct ListModelOptional<O>(PhantomData<O>);

impl<O: IsA<glib::Object>> ListModelOptional<O> {
    pub fn static_variant_type() -> Cow<'static, VariantTy>
    where
        O: glib::StaticVariantType,
    {
        let mut builder = glib::GStringBuilder::new("ma");
        builder.append(O::static_variant_type().as_str());
        Cow::Owned(VariantType::from_string(builder.into_string()).unwrap())
    }
    pub fn to_variant<M>(m: &Option<M>) -> Variant
    where
        M: IsA<gio::ListModel>,
        O: glib::StaticVariantType + ToVariant,
    {
        match m.as_ref() {
            Some(value) => Variant::from_some(&ListModel::<O>::to_variant(value)),
            None => Variant::from_none(&*Self::static_variant_type()),
        }
    }
}

pub struct ListStore<O>(PhantomData<O>);

impl<O: IsA<glib::Object>> ListStore<O> {
    pub fn static_variant_type() -> Cow<'static, VariantTy>
    where
        O: glib::StaticVariantType,
    {
        ListModel::<O>::static_variant_type()
    }
    pub fn to_variant(ls: &gio::ListStore) -> Variant
    where
        O: glib::StaticVariantType + ToVariant,
    {
        ListModel::<O>::to_variant(ls)
    }
    pub fn from_variant(variant: &Variant) -> Option<gio::ListStore>
    where
        O: glib::FromVariant,
    {
        if !variant.is_type(VariantTy::ARRAY) {
            return None;
        }
        if variant.type_().element() != O::static_variant_type() {
            return None;
        }
        let store = gio::ListStore::new(O::static_type());
        for variant in variant.iter() {
            if let Some(o) = variant.get::<O>() {
                store.append(&o);
            }
        }
        Some(store)
    }
}

pub struct ListStoreOptional<O>(PhantomData<O>);

impl<O: IsA<glib::Object>> ListStoreOptional<O> {
    pub fn static_variant_type() -> Cow<'static, VariantTy>
    where
        O: glib::StaticVariantType,
    {
        ListModelOptional::<O>::static_variant_type()
    }
    pub fn to_variant(ls: &Option<gio::ListStore>) -> Variant
    where
        O: glib::StaticVariantType + ToVariant,
    {
        ListModelOptional::<O>::to_variant(ls)
    }
    pub fn from_variant(variant: &Variant) -> Option<Option<gio::ListStore>>
    where
        O: glib::FromVariant,
    {
        if !variant.is_type(&*Self::static_variant_type()) {
            return None;
        }
        match variant.as_maybe() {
            Some(variant) => Some(Some(ListStore::<O>::from_variant(&variant)?)),
            None => Some(None),
        }
    }
}
