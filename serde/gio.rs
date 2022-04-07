use gio::prelude::*;
use glib::IsA;
use serde::{
    de,
    ser::{self, SerializeSeq},
    Deserialize, Deserializer, Serialize, Serializer,
};
use std::marker::PhantomData;

pub mod file {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gio::File")]
    struct FileUri<'f>(&'f str);
    pub fn serialize<S: Serializer>(f: &gio::File, s: S) -> Result<S::Ok, S::Error> {
        FileUri(f.uri().as_str()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gio::File, D::Error> {
        Ok(gio::File::for_uri(FileUri::deserialize(d)?.0))
    }
}

pub mod icon {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gio::Icon")]
    struct Icon<'i>(&'i str);
    pub fn serialize<S: Serializer>(i: &gio::Icon, s: S) -> Result<S::Ok, S::Error> {
        let i = IconExt::to_string(i)
            .ok_or_else(|| ser::Error::custom("GIcon cannot be serialized"))?;
        Icon(i.as_str()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gio::Icon, D::Error> {
        let Icon(s) = Icon::deserialize(d)?;
        gio::Icon::for_string(s).map_err(|e| de::Error::custom(e.message()))
    }
}

pub struct ListModel<O>(PhantomData<O>);

impl<O: IsA<glib::Object> + Serialize> ListModel<O> {
    pub fn serialize<M, S>(m: &M, s: S) -> Result<S::Ok, S::Error>
    where
        M: IsA<gio::ListModel>,
        S: Serializer,
    {
        let count = m.n_items();
        let mut seq = s.serialize_seq(Some(count as usize))?;
        for i in 0..count {
            let o = m.item(i).ok_or_else(|| {
                ser::Error::custom(format!("Unexpected end of ListModel at index {}", i))
            })?;
            let o = o.downcast::<O>().map_err(|o| {
                ser::Error::custom(format!(
                    "Wrong type for ListModel index {}: Expected `{}` got `{}`",
                    i,
                    O::static_type().name(),
                    o.type_().name(),
                ))
            })?;
            seq.serialize_element(&o)?;
        }
        seq.end()
    }
}

pub struct ListStore<O>(PhantomData<O>);

impl<O: IsA<glib::Object>> ListStore<O> {
    pub fn serialize<S: Serializer>(ls: &gio::ListStore, s: S) -> Result<S::Ok, S::Error>
    where
        O: Serialize,
    {
        ListModel::<O>::serialize(ls, s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gio::ListStore, D::Error>
    where
        O: serde::Deserialize<'de>,
    {
        struct Visitor<'de, O: IsA<glib::Object> + serde::Deserialize<'de>>(PhantomData<&'de O>);

        impl<'de, O> de::Visitor<'de> for Visitor<'de, O>
        where
            O: IsA<glib::Object> + serde::Deserialize<'de>,
        {
            type Value = gio::ListStore;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a sequence")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let store = gio::ListStore::new(O::static_type());

                while let Some(value) = seq.next_element::<O>()? {
                    store.append(&value);
                }

                Ok(store)
            }
        }

        d.deserialize_seq(Visitor::<O>(PhantomData))
    }
}
