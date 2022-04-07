use glib::once_cell::sync::OnceCell as SyncOnceCell;
use glib::translate::*;
use serde::de::VariantAccess;
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::Display;

#[path = "variant.rs"]
pub mod variant;

pub mod enum_ {
    use super::*;
    use glib::{EnumClass, EnumValue};

    pub fn serialize<E, S: Serializer>(e: &E, s: S) -> Result<S::Ok, S::Error>
    where
        E: glib::StaticType + IntoGlib<GlibType = i32> + Copy,
    {
        serialize_for_type(E::static_type(), e.into_glib(), s)
    }
    #[inline]
    fn serialize_for_type<S: Serializer>(t: glib::Type, e: i32, s: S) -> Result<S::Ok, S::Error> {
        let class = EnumClass::new(t)
            .ok_or_else(|| ser::Error::custom(format!("GType `{}` is not an enum", t.name())))?;
        let n = class.value(e).map(|e| e.nick()).ok_or_else(|| {
            ser::Error::custom(format!("Invalid value `{}` for enum `{}`", e, t.name()))
        })?;
        let n = unsafe { std::mem::transmute(n) };
        s.serialize_unit_variant(t.name(), e as u32, n)
    }
    pub fn deserialize<'de, E, D: Deserializer<'de>>(d: D) -> Result<E, D::Error>
    where
        E: glib::StaticType + FromGlib<i32>,
    {
        let e = deserialize_for_type(E::static_type(), d)?;
        Ok(unsafe { from_glib(e) })
    }
    #[inline]
    fn deserialize_for_type<'de, D: Deserializer<'de>>(
        t: glib::Type,
        d: D,
    ) -> Result<i32, D::Error> {
        let class = glib::EnumClass::new(t)
            .ok_or_else(|| de::Error::custom(format!("GType `{}` is not an enum", t.name())))?;

        static VARIANTS: SyncOnceCell<Vec<&'static str>> = SyncOnceCell::new();

        let variants = VARIANTS.get_or_init(|| {
            class
                .values()
                .iter()
                .map(|v| unsafe { std::mem::transmute(v.name()) })
                .collect()
        });

        struct FieldVisitor<'e>(&'e EnumClass);
        impl<'de, 'e> de::Visitor<'de> for FieldVisitor<'e> {
            type Value = &'e EnumValue;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                "enum identifier".fmt(f)
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                i32::try_from(v)
                    .ok()
                    .and_then(|v| self.0.value(v))
                    .ok_or_else(|| {
                        let indices = self
                            .0
                            .values()
                            .iter()
                            .map(|v| v.value().to_string())
                            .collect::<Vec<_>>()
                            .join(", ");
                        de::Error::invalid_value(
                            serde::de::Unexpected::Unsigned(v as u64),
                            &indices.as_str(),
                        )
                    })
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                self.0
                    .value_by_nick(v)
                    .ok_or_else(|| de::Error::unknown_variant(v, VARIANTS.get().unwrap()))
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                std::str::from_utf8(v)
                    .ok()
                    .and_then(|s| self.0.value_by_nick(s))
                    .ok_or_else(|| {
                        let v = String::from_utf8_lossy(v);
                        de::Error::unknown_variant(&v, VARIANTS.get().unwrap())
                    })
            }
        }
        impl<'de, 'e> serde::de::DeserializeSeed<'de> for FieldVisitor<'e> {
            type Value = &'e EnumValue;
            fn deserialize<D>(self, d: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                d.deserialize_identifier(self)
            }
        }

        struct Visitor(EnumClass);
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = i32;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                self.0.type_().name().fmt(f)
            }
            fn visit_enum<A>(self, data: A) -> Result<Self::Value, A::Error>
            where
                A: de::EnumAccess<'de>,
            {
                let vis = FieldVisitor(&self.0);
                let (e, v) = data.variant_seed(vis)?;
                v.unit_variant()?;
                Ok(e.value())
            }
        }

        d.deserialize_enum(t.name(), variants.as_slice(), Visitor(class))
    }
}

pub mod flags {
    use super::*;

    pub fn serialize<F, S: Serializer>(f: &F, s: S) -> Result<S::Ok, S::Error>
    where
        F: glib::StaticType + IntoGlib<GlibType = u32> + Copy,
    {
        let t = F::static_type();
        let f = f.into_glib();
        s.serialize_newtype_struct(t.name(), &f)
    }
    pub fn deserialize<'de, F, D: Deserializer<'de>>(d: D) -> Result<F, D::Error>
    where
        F: glib::StaticType + FromGlib<u32>,
    {
        let v = deserialize_for_type(F::static_type(), d)?;
        Ok(unsafe { from_glib(v) })
    }
    #[inline]
    fn deserialize_for_type<'de, D: Deserializer<'de>>(
        t: glib::Type,
        d: D,
    ) -> Result<u32, D::Error> {
        struct Visitor(glib::Type);
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = u32;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                self.0.name().fmt(f)
            }
            fn visit_newtype_struct<D>(self, d: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                <u32 as Deserialize>::deserialize(d)
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                seq.next_element::<u32>()?.ok_or_else(|| {
                    de::Error::invalid_length(
                        0,
                        &format!("tuple struct {} with length 1", self.0.name()).as_str(),
                    )
                })
            }
        }

        d.deserialize_newtype_struct(t.name(), Visitor(t))
    }
}

pub mod gstr {
    use super::*;
    pub fn serialize<S: Serializer>(st: &glib::GStr, s: S) -> Result<S::Ok, S::Error> {
        st.as_str().serialize(s)
    }
}

pub mod gstring {
    use super::*;
    pub fn serialize<S: Serializer>(st: &glib::GString, s: S) -> Result<S::Ok, S::Error> {
        st.as_str().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::GString, D::Error> {
        Ok(String::deserialize(d)?.into())
    }
}

pub mod bytes {
    use super::*;
    pub fn serialize<S: Serializer>(b: &glib::Bytes, s: S) -> Result<S::Ok, S::Error> {
        b.as_ref().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::Bytes, D::Error> {
        struct BytesVisitor;
        impl<'de> de::Visitor<'de> for BytesVisitor {
            type Value = glib::Bytes;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a byte array")
            }
            fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
                Ok(v.into())
            }
            fn visit_byte_buf<E: de::Error>(self, v: Vec<u8>) -> Result<Self::Value, E> {
                Ok(glib::Bytes::from_owned(v))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(v.as_bytes().into())
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(glib::Bytes::from_owned(v))
            }
        }
        d.deserialize_bytes(BytesVisitor)
    }
}

pub mod date {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "glib::Date")]
    struct Date(u32);
    pub fn serialize<S: Serializer>(d: &glib::Date, s: S) -> Result<S::Ok, S::Error> {
        Date(d.julian()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::Date, D::Error> {
        glib::Date::from_julian(Date::deserialize(d)?.0).map_err(de::Error::custom)
    }
}

pub mod time_zone {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "glib::TimeZone")]
    struct TimeZone<'t>(&'t str);
    //#[cfg(feature = "glib/v2_68")]
    pub fn serialize<S: Serializer>(tz: &glib::TimeZone, s: S) -> Result<S::Ok, S::Error> {
        TimeZone(tz.identifier().as_str()).serialize(s)
    }
    /*
    #[cfg(not(feature = "glib/v2_68"))]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::TimeZone, D::Error> {
        Ok(glib::TimeZone::new(Some(TimeZone::deserialize(d)?.0)))
    }
    */
    //#[cfg(feature = "glib/v2_68")]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::TimeZone, D::Error> {
        let s = TimeZone::deserialize(d)?.0;
        glib::TimeZone::from_identifier(Some(s))
            .ok_or_else(|| de::Error::invalid_value(de::Unexpected::Str(s), &"valid timezone"))
    }
}

pub mod date_time {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "glib::DateTime")]
    struct DateTime<'d>(&'d str);
    //#[cfg(feature = "glib/v2_56")]
    pub fn serialize<S: Serializer>(dt: &glib::DateTime, s: S) -> Result<S::Ok, S::Error> {
        DateTime(dt.format_iso8601().map_err(ser::Error::custom)?.as_str()).serialize(s)
    }
    //#[cfg(feature = "glib/v2_56")]
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::DateTime, D::Error> {
        glib::DateTime::from_iso8601(DateTime::deserialize(d)?.0, None).map_err(de::Error::custom)
    }
}

pub mod time_span {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "glib::TimeSpan")]
    struct TimeSpan(i64);
    pub fn serialize<S: Serializer>(ts: &glib::TimeSpan, s: S) -> Result<S::Ok, S::Error> {
        TimeSpan(ts.0).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::TimeSpan, D::Error> {
        Ok(glib::TimeSpan(TimeSpan::deserialize(d)?.0))
    }
}

pub mod key_file {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "glib::KeyFile")]
    struct KeyFile<'k>(&'k str);
    pub fn serialize<S: Serializer>(kf: &glib::KeyFile, s: S) -> Result<S::Ok, S::Error> {
        KeyFile(kf.to_data().as_str()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::KeyFile, D::Error> {
        let d = KeyFile::deserialize(d)?.0;
        let kf = glib::KeyFile::new();
        glib::KeyFile::load_from_data(&kf, d, glib::KeyFileFlags::NONE)
            .map_err(de::Error::custom)?;
        Ok(kf)
    }
}

pub mod uri {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "glib::Uri")]
    struct Uri<'u>(&'u str);
    pub fn serialize<S: Serializer>(u: &glib::Uri, s: S) -> Result<S::Ok, S::Error> {
        Uri(u.to_str().as_str()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::Uri, D::Error> {
        let u = Uri::deserialize(d)?.0;
        glib::Uri::parse(u, glib::UriFlags::PARSE_RELAXED).map_err(de::Error::custom)
    }
}
