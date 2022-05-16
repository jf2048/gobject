use crate::variant::glib::bytes;
use glib::{ToVariant, Variant, VariantTy};
use std::{borrow::Cow, marker::PhantomData};

pub mod rectangle {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(iiii)") })
    }
    pub fn to_variant(r: &gdk4::Rectangle) -> Variant {
        (r.x(), r.y(), r.width(), r.height()).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gdk4::Rectangle> {
        let (x, y, w, h) = variant.get()?;
        Some(gdk4::Rectangle::new(x, y, w, h))
    }
    declare_optional!(gdk4::Rectangle);
}

pub mod rgba {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddd)") })
    }
    pub fn to_variant(c: &gdk4::RGBA) -> Variant {
        (
            c.red() as f64,
            c.green() as f64,
            c.blue() as f64,
            c.alpha() as f64,
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gdk4::RGBA> {
        let (r, g, b, a) = variant.get::<(f64, f64, f64, f64)>()?;
        Some(gdk4::RGBA::new(r as f32, g as f32, b as f32, a as f32))
    }
    declare_optional!(gdk4::RGBA);
}

pub trait MimeType {
    fn mime_type() -> Cow<'static, str>;
}

pub struct Content<T, M: MimeType>(PhantomData<T>, PhantomData<M>);

impl<T, M: MimeType> Content<T, M> {
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        bytes::static_variant_type()
    }
    pub fn to_variant(c: &T) -> Variant
    where
        T: glib::ToValue,
    {
        use gdk4::gio::prelude::*;
        let stream = gdk4::gio::MemoryOutputStream::new_resizable();
        glib::MainContext::ref_thread_default()
            .block_on(async {
                gdk4::content_serialize_future(
                    &stream,
                    &*M::mime_type(),
                    &c.to_value(),
                    glib::PRIORITY_DEFAULT,
                )
                .await?;
                stream.close_future(glib::PRIORITY_DEFAULT).await
            })
            .unwrap();
        stream.steal_as_bytes().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<T>
    where
        T: glib::value::ValueType,
    {
        use gdk4::gio::prelude::*;
        let bytes = bytes::from_variant(variant)?;
        let stream = gdk4::gio::MemoryInputStream::from_bytes(&bytes);
        glib::MainContext::ref_thread_default()
            .block_on(async {
                let value = gdk4::content_deserialize_future(
                    &stream,
                    &*M::mime_type(),
                    T::Type::static_type(),
                    glib::PRIORITY_DEFAULT,
                )
                .await?;
                stream.close_future(glib::PRIORITY_DEFAULT).await?;
                Ok::<glib::Value, glib::Error>(value)
            })
            .ok()?
            .get()
            .ok()
    }
}

pub struct ContentOptional<T, M: MimeType>(PhantomData<T>, PhantomData<M>);

impl<T, M: MimeType> ContentOptional<T, M> {
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("may") })
    }
    pub fn to_variant(c: &Option<T>) -> Variant
    where
        T: glib::ToValue,
    {
        match c.as_ref() {
            Some(value) => Variant::from_some(&Content::<T, M>::to_variant(value)),
            None => Variant::from_none(&*Content::<T, M>::static_variant_type()),
        }
    }
    pub fn from_variant(variant: &Variant) -> Option<Option<T>>
    where
        T: glib::value::ValueType,
    {
        if !variant.is_type(&*Self::static_variant_type()) {
            return None;
        }
        match variant.as_maybe() {
            Some(variant) => Some(Some(Content::<T, M>::from_variant(&variant)?)),
            None => Some(None),
        }
    }
}
