use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::{borrow::Cow, marker::PhantomData};

pub mod rectangle {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gdk4::Rectangle")]
    struct Rectangle(i32, i32, i32, i32);
    pub fn serialize<S: Serializer>(r: &gdk4::Rectangle, s: S) -> Result<S::Ok, S::Error> {
        Rectangle(r.x(), r.y(), r.width(), r.height()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gdk4::Rectangle, D::Error> {
        let Rectangle(x, y, w, h) = Rectangle::deserialize(d)?;
        Ok(gdk4::Rectangle::new(x, y, w, h))
    }
    declare_optional!(gdk4::Rectangle);
}

pub mod rgba {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gdk4::RGBA")]
    struct RGBA(f32, f32, f32, f32);
    pub fn serialize<S: Serializer>(c: &gdk4::RGBA, s: S) -> Result<S::Ok, S::Error> {
        RGBA(c.red(), c.green(), c.blue(), c.alpha()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gdk4::RGBA, D::Error> {
        let RGBA(r, g, b, a) = RGBA::deserialize(d)?;
        Ok(gdk4::RGBA::new(r, g, b, a))
    }
    declare_optional!(gdk4::RGBA);
}

pub trait MimeType {
    fn mime_type() -> Cow<'static, str>;
}

pub struct Content<T, M: MimeType>(PhantomData<T>, PhantomData<M>);

impl<T, M: MimeType> Content<T, M> {
    pub fn serialize<S: Serializer>(c: &T, s: S) -> Result<S::Ok, S::Error>
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
        stream.steal_as_bytes().as_ref().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<T, D::Error>
    where
        T: glib::value::ValueType,
    {
        use gdk4::gio::prelude::*;
        let bytes = crate::glib::bytes::deserialize(d)?;
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
            .map_err(de::Error::custom)?
            .get()
            .map_err(de::Error::custom)
    }
}

pub struct ContentOptional<T, M: MimeType>(PhantomData<T>, PhantomData<M>);

impl<T, M: MimeType> ContentOptional<T, M> {
    pub fn serialize<S: Serializer>(c: &Option<T>, s: S) -> Result<S::Ok, S::Error>
    where
        T: glib::ToValue,
    {
        #[derive(serde::Serialize)]
        #[serde(transparent)]
        struct Writer<'w, T: glib::ToValue, M: MimeType>(
            #[serde(with = "Content::<T, M>")] &'w T,
            #[serde(skip)] PhantomData<M>,
        );
        serde::Serialize::serialize(&c.as_ref().map(|c| Writer::<T, M>(c, PhantomData)), s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<T>, D::Error>
    where
        T: glib::value::ValueType,
    {
        #[derive(serde::Deserialize)]
        #[serde(transparent)]
        struct Reader<T: glib::value::ValueType, M: MimeType>(
            #[serde(with = "Content::<T, M>")] T,
            #[serde(skip)] PhantomData<M>,
        );
        <Option<Reader<T, M>> as serde::Deserialize>::deserialize(d).map(|o| o.map(|o| o.0))
    }
}
