use glib::translate::*;
use glib::{StaticType, ToVariant, Variant, VariantTy};
use std::borrow::Cow;

pub mod enum_ {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::INT32)
    }
    pub fn to_variant<E>(e: &E) -> Variant
    where
        E: StaticType + IntoGlib<GlibType = i32> + Copy,
    {
        e.into_glib().to_variant()
    }
    pub fn from_variant<E>(variant: &Variant) -> Option<E>
    where
        E: glib::StaticType + FromGlib<i32>,
    {
        let v = variant.get()?;
        Some(unsafe { from_glib(v) })
    }
}

pub mod enum_string {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant<E>(e: &E) -> Variant
    where
        E: StaticType + IntoGlib<GlibType = i32> + Copy,
    {
        let class = glib::EnumClass::new(E::static_type())
            .unwrap_or_else(|| panic!("GType `{}` is not an enum class", E::static_type().name()));
        let e = e.into_glib();
        let n = class.value(e).map(|e| e.nick()).unwrap_or_else(|| {
            panic!(
                "Invalid value `{}` for enum `{}`",
                e,
                E::static_type().name()
            )
        });
        n.to_variant()
    }
    pub fn from_variant<E>(variant: &Variant) -> Option<E>
    where
        E: glib::StaticType + FromGlib<i32>,
    {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        let class = glib::EnumClass::new(E::static_type())?;
        let v = class.value_by_nick(variant.str()?)?.value();
        Some(unsafe { from_glib(v) })
    }
}

pub mod flags {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::UINT32)
    }
    pub fn to_variant<F>(f: &F) -> Variant
    where
        F: StaticType + IntoGlib<GlibType = u32> + Copy,
    {
        f.into_glib().to_variant()
    }
    pub fn from_variant<F>(variant: &Variant) -> Option<F>
    where
        F: glib::StaticType + FromGlib<u32>,
    {
        let v = variant.get()?;
        Some(unsafe { from_glib(v) })
    }
}

pub mod flags_string {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant<F>(f: &F) -> Variant
    where
        F: StaticType + IntoGlib<GlibType = u32> + Copy,
    {
        let class = glib::FlagsClass::new(F::static_type())
            .unwrap_or_else(|| panic!("GType `{}` is not an enum class", F::static_type().name()));
        class.to_nick_string(f.into_glib()).to_variant()
    }
    pub fn from_variant<F>(variant: &Variant) -> Option<F>
    where
        F: glib::StaticType + FromGlib<u32>,
    {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        let class = glib::FlagsClass::new(F::static_type())?;
        let v = class.from_nick_string(variant.str()?).ok()?;
        Some(unsafe { from_glib(v) })
    }
}

pub mod gstr {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(st: &glib::GStr) -> Variant {
        st.as_str().to_variant()
    }
}

pub mod gstring {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(st: &glib::GString) -> Variant {
        st.as_str().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::GString> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        Some(glib::GString::from(variant.str()?))
    }
}

pub mod bytes {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::BYTE_STRING)
    }
    pub fn to_variant(b: &glib::Bytes) -> Variant {
        b.to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::Bytes> {
        Some(glib::Bytes::from(variant.fixed_array().ok()?))
    }
}

pub mod date {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::UINT32)
    }
    pub fn to_variant(d: &glib::Date) -> Variant {
        d.julian().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::Date> {
        glib::Date::from_julian(variant.get()?).ok()
    }
}

pub mod time_zone {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(t: &glib::TimeZone) -> Variant {
        t.identifier().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::TimeZone> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        glib::TimeZone::from_identifier(Some(variant.str()?))
    }
}

pub mod date_time {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(dt: &glib::DateTime) -> Variant {
        dt.format_iso8601()
            .unwrap_or_else(|_| "".into())
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::DateTime> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        glib::DateTime::from_iso8601(variant.str()?, None).ok()
    }
}

pub mod time_span {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::INT64)
    }
    pub fn to_variant(ts: &glib::TimeSpan) -> Variant {
        ts.0.to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::TimeSpan> {
        Some(glib::TimeSpan(variant.get()?))
    }
}

pub mod key_file {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(kf: &glib::KeyFile) -> Variant {
        kf.to_data().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::KeyFile> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        let d = variant.str()?;
        let kf = glib::KeyFile::new();
        glib::KeyFile::load_from_data(&kf, d, glib::KeyFileFlags::NONE).ok()?;
        Some(kf)
    }
}

pub mod uri {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(uri: &glib::Uri) -> Variant {
        uri.to_str().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<glib::Uri> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        let u = variant.str()?;
        glib::Uri::parse(u, glib::UriFlags::PARSE_RELAXED).ok()
    }
}
