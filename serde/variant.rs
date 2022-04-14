use glib::ffi;
use glib::translate::*;
use glib::BoolError;
use glib::{variant::VariantTypeMismatchError, ToVariant, VariantClass, VariantTy};
use serde::{
    de::{self, DeserializeSeed, Visitor},
    Deserialize, Deserializer,
};
use serde::{
    ser::{self, SerializeMap, SerializeSeq, SerializeTuple, SerializeTupleStruct},
    Serialize, Serializer,
};
use std::{fmt::Display, num::TryFromIntError};
use std::{marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

declare_optional!(glib::Variant);

#[derive(Debug)]
enum Error {
    Bool(BoolError),
    Mismatch(VariantTypeMismatchError),
    Int(TryFromIntError),
    UnsupportedType(glib::VariantType),
    Custom(String),
}

impl Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bool(e) => e.fmt(f),
            Self::Mismatch(e) => e.fmt(f),
            Self::Int(e) => e.fmt(f),
            Self::UnsupportedType(actual) => {
                write!(f, "Type not supported: '{}'", actual)
            }
            Self::Custom(e) => e.fmt(f),
        }
    }
}

impl From<BoolError> for Error {
    fn from(e: BoolError) -> Self {
        Self::Bool(e)
    }
}

impl From<VariantTypeMismatchError> for Error {
    fn from(e: VariantTypeMismatchError) -> Self {
        Self::Mismatch(e)
    }
}

impl From<TryFromIntError> for Error {
    fn from(e: TryFromIntError) -> Self {
        Self::Int(e)
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Bool(e) => Some(e),
            Self::Mismatch(e) => Some(e),
            Self::Int(e) => Some(e),
            _ => None,
        }
    }
}

impl serde::de::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Error::Custom(msg.to_string())
    }
}

impl serde::ser::Error for Error {
    fn custom<T>(msg: T) -> Self
    where
        T: Display,
    {
        Error::Custom(msg.to_string())
    }
}

#[repr(transparent)]
pub(super) struct VariantWrapper(pub(super) glib::Variant);

impl Serialize for VariantWrapper {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        serialize(&self.0, s)
    }
}

impl<'de> Deserialize<'de> for VariantWrapper {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        deserialize(d).map(Self)
    }
}

pub fn serialize<S: Serializer>(v: &glib::Variant, s: S) -> Result<S::Ok, S::Error> {
    let mut tuple = s.serialize_tuple_struct("glib::Variant", 2)?;
    tuple.serialize_field(v.type_().as_str())?;
    tuple.serialize_field(&VariantInner(v))?;
    tuple.end()
}

struct VariantInner<'t>(&'t glib::Variant);

impl<'t> Serialize for VariantInner<'t> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        #[inline]
        fn try_serialize<T, S>(v: &glib::Variant, s: S) -> Result<S::Ok, S::Error>
        where
            T: glib::FromVariant + Serialize,
            S: ser::Serializer,
        {
            v.try_get::<T>().map_err(ser::Error::custom)?.serialize(s)
        }

        let v = self.0;
        match v.classify() {
            VariantClass::Boolean => try_serialize::<bool, _>(v, s),
            VariantClass::Byte => try_serialize::<u8, _>(v, s),
            VariantClass::Int16 => try_serialize::<i16, _>(v, s),
            VariantClass::Uint16 => try_serialize::<u16, _>(v, s),
            VariantClass::Int32 => try_serialize::<i32, _>(v, s),
            VariantClass::Uint32 => try_serialize::<u32, _>(v, s),
            VariantClass::Int64 => try_serialize::<i64, _>(v, s),
            VariantClass::Uint64 => try_serialize::<u64, _>(v, s),
            VariantClass::Handle => Err(ser::Error::custom("HANDLE values not supported")),
            VariantClass::Double => try_serialize::<f64, _>(v, s),
            VariantClass::String => v.str().unwrap().serialize(s),
            VariantClass::ObjectPath => {
                s.serialize_newtype_struct("glib::ObjectPath", v.str().unwrap())
            }
            VariantClass::Signature => {
                s.serialize_newtype_struct("glib::Signature", v.str().unwrap())
            }
            VariantClass::Variant => VariantWrapper(v.as_variant().unwrap()).serialize(s),
            VariantClass::Maybe => match v.as_maybe() {
                Some(inner) => s.serialize_some(&VariantWrapper(inner)),
                None => s.serialize_none(),
            },
            VariantClass::Array => {
                let count = v.n_children();
                let child_type = v.type_().element();
                if child_type.is_dict_entry() {
                    let mut seq = s.serialize_map(Some(count))?;
                    for i in 0..count {
                        let entry = v.child_value(i);
                        let key = entry.child_value(0);
                        let value = entry.child_value(1);
                        seq.serialize_entry(&VariantWrapper(key), &VariantWrapper(value))?;
                    }
                    seq.end()
                } else if child_type == VariantTy::BYTE {
                    s.serialize_bytes(v.fixed_array().map_err(ser::Error::custom)?)
                } else {
                    let mut seq = s.serialize_seq(Some(count))?;
                    for i in 0..count {
                        let child = v.child_value(i);
                        seq.serialize_element(&VariantWrapper(child))?;
                    }
                    seq.end()
                }
            }
            VariantClass::Tuple => {
                let count = v.n_children();
                if count > 0 {
                    let mut seq = s.serialize_tuple(count)?;
                    for i in 0..count {
                        let child = v.child_value(i);
                        seq.serialize_element(&VariantWrapper(child))?;
                    }
                    seq.end()
                } else {
                    s.serialize_unit()
                }
            }
            VariantClass::DictEntry => Err(ser::Error::custom("DICT_ENTRY values not supported")),
            _ => Err(ser::Error::custom("Unknown variant type")),
        }
    }
}

pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<glib::Variant, D::Error> {
    d.deserialize_tuple_struct("glib::Variant", 2, VariantVisitor)
        .map(Into::into)
}

struct VariantVisitor;

impl<'de> Visitor<'de> for VariantVisitor {
    type Value = glib::Variant;

    fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("GVariant tuple of length 2")
    }
    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let tag = seq
            .next_element::<&str>()?
            .ok_or_else(|| de::Error::invalid_length(0, &"tuple struct Variant with 2 elements"))?;
        let ty = VariantTy::new(tag).map_err(de::Error::custom)?;
        if !ty.is_definite() {
            return Err(de::Error::custom("Type must be definite"));
        }
        let seed = VariantSeed(ty);
        let value = seq
            .next_element_seed(seed)?
            .ok_or_else(|| de::Error::invalid_length(1, &"tuple struct Variant with 2 elements"))?;
        Ok(value)
    }
}

#[repr(transparent)]
struct VariantSeed<'t>(&'t VariantTy);

impl<'t, 'de> DeserializeSeed<'de> for VariantSeed<'t> {
    type Value = glib::Variant;

    fn deserialize<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let ty = self.0;
        let visitor = VariantValueVisitor(ty);
        if ty.is_basic() {
            match ty.as_str() {
                "b" => deserializer.deserialize_bool(visitor),
                "y" => deserializer.deserialize_u8(visitor),
                "n" => deserializer.deserialize_i16(visitor),
                "q" => deserializer.deserialize_u16(visitor),
                "i" => deserializer.deserialize_i32(visitor),
                "u" => deserializer.deserialize_u32(visitor),
                "x" => deserializer.deserialize_i64(visitor),
                "t" => deserializer.deserialize_u64(visitor),
                "d" => deserializer.deserialize_f64(visitor),
                "s" => deserializer.deserialize_str(visitor),
                "o" => deserializer.deserialize_newtype_struct("glib::ObjectPath", visitor),
                "g" => deserializer.deserialize_newtype_struct("glib::Signature", visitor),
                "h" => Err(de::Error::custom("HANDLE values not supported")),
                _ => unimplemented!(),
            }
        } else if ty.is_array() {
            let elem = ty.element();
            if elem == VariantTy::BYTE {
                deserializer.deserialize_bytes(visitor)
            } else if ty.element().is_dict_entry() {
                deserializer.deserialize_map(visitor)
            } else {
                deserializer.deserialize_seq(visitor)
            }
        } else if ty.is_tuple() {
            let len = ty.n_items();
            if len > 0 {
                deserializer.deserialize_tuple(len, visitor)
            } else {
                deserializer.deserialize_unit(visitor)
            }
        } else if ty.is_maybe() {
            deserializer.deserialize_option(visitor)
        } else if ty.is_variant() {
            deserialize(deserializer).map(|v| v.to_variant())
        } else {
            Err(de::Error::custom(Error::UnsupportedType(ty.to_owned())))
        }
    }
}

#[repr(transparent)]
struct VariantValueVisitor<'t>(&'t VariantTy);

impl<'t, 'de> Visitor<'de> for VariantValueVisitor<'t> {
    type Value = glib::Variant;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("any valid GVariant value")
    }

    #[inline]
    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(value.to_variant())
    }

    #[inline]
    fn visit_i16<E: de::Error>(self, v: i16) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    #[inline]
    fn visit_i32<E: de::Error>(self, v: i32) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    #[inline]
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    #[inline]
    fn visit_u8<E: de::Error>(self, v: u8) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    #[inline]
    fn visit_u16<E: de::Error>(self, v: u16) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    #[inline]
    fn visit_u32<E: de::Error>(self, v: u32) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    #[inline]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    #[inline]
    fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
        Ok(v.to_variant())
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
        match self.0.as_str() {
            "s" => Ok(v.to_variant()),
            "o" => {
                let p = v.to_glib_none();
                let valid = unsafe { ffi::g_variant_is_object_path(p.0) };
                if valid == ffi::GFALSE {
                    return Err(de::Error::invalid_value(
                        de::Unexpected::Str(v),
                        &"valid object path",
                    ));
                }
                let v = unsafe { from_glib_none(ffi::g_variant_new_object_path(p.0)) };
                Ok(v)
            }
            "g" => {
                let p = v.to_glib_none();
                let valid = unsafe { ffi::g_variant_is_signature(p.0) };
                if valid == ffi::GFALSE {
                    return Err(de::Error::invalid_value(
                        de::Unexpected::Str(v),
                        &"valid D-Bus signature",
                    ));
                }
                let v = unsafe { from_glib_none(ffi::g_variant_new_signature(p.0)) };
                Ok(v)
            }
            _ => Err(de::Error::custom(Error::UnsupportedType(self.0.to_owned()))),
        }
    }

    fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
        match self.0.as_str() {
            "s" => Ok(v.to_variant()),
            _ => self.visit_str(&v),
        }
    }

    #[inline]
    fn visit_bytes<E: de::Error>(self, v: &[u8]) -> Result<Self::Value, E> {
        Ok(glib::Variant::array_from_fixed_array(v))
    }

    #[inline]
    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(glib::Variant::from_none(self.0.element()))
    }

    fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        let seed = VariantSeed(self.0.element());
        let value = seed.deserialize(deserializer)?;
        Ok(glib::Variant::from_some(&value))
    }

    #[inline]
    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(().to_variant())
    }

    fn visit_newtype_struct<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_string(self)
    }

    fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
    where
        A: de::SeqAccess<'de>,
    {
        let ty = self.0;
        if ty.is_array() {
            let builder = VariantBuilder::new(ty);
            let elem = self.0.element();
            while let Some(value) = {
                let seed = VariantSeed(elem);
                seq.next_element_seed(seed)?
            } {
                check_type(&value, elem).map_err(de::Error::custom)?;
                unsafe {
                    builder.add_value(&value);
                }
            }
            Ok(builder.end())
        } else if ty.is_tuple() || ty.is_dict_entry() {
            let builder = VariantBuilder::new(ty);
            let len = ty.n_items();
            let mut iter = ty.first();
            for i in 0..len {
                let elem = iter.unwrap();
                let seed = VariantSeed(elem);
                let value = seq.next_element_seed(seed)?.ok_or_else(|| {
                    de::Error::invalid_length(i, &format!("tuple of length {}", len).as_str())
                })?;
                check_type(&value, elem).map_err(de::Error::custom)?;
                unsafe {
                    builder.add_value(&value);
                }
                iter = elem.next();
            }
            Ok(builder.end())
        } else if ty.is_definite() {
            let seed = VariantSeed(ty);
            seq.next_element_seed(seed)?
                .ok_or_else(|| de::Error::invalid_length(0, &"tuple of length 1"))
        } else {
            Err(de::Error::custom(Error::UnsupportedType(ty.to_owned())))
        }
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: de::MapAccess<'de>,
    {
        if !self.0.is_array() || !self.0.element().is_dict_entry() {
            return Err(de::Error::custom(Error::UnsupportedType(self.0.to_owned())));
        }
        let builder = VariantBuilder::new(self.0);
        let elem = self.0.element();
        let key_type = elem.key();
        let value_type = elem.value();
        while let Some((key, value)) = {
            let kseed = VariantSeed(key_type);
            let vseed = VariantSeed(value_type);
            map.next_entry_seed(kseed, vseed)?
        } {
            let dict_entry = builder.open(self.0.element());
            check_type(&key, key_type).map_err(de::Error::custom)?;
            check_type(&value, value_type).map_err(de::Error::custom)?;
            unsafe {
                dict_entry.add_value(&key);
                dict_entry.add_value(&value);
            }
        }
        Ok(builder.end())
    }
}

#[inline]
fn check_type(variant: &glib::Variant, ty: &VariantTy) -> Result<(), VariantTypeMismatchError> {
    if !variant.is_type(ty) {
        return Err(VariantTypeMismatchError::new(
            variant.type_().to_owned(),
            ty.to_owned(),
        ));
    }
    Ok(())
}

struct VariantBuilder(ffi::GVariantBuilder);

impl VariantBuilder {
    pub fn new(ty: &VariantTy) -> Self {
        let mut builder: MaybeUninit<ffi::GVariantBuilder> = MaybeUninit::uninit();
        Self(unsafe {
            ffi::g_variant_builder_init(builder.as_mut_ptr(), ty.to_glib_none().0);
            builder.assume_init()
        })
    }
    pub fn end(self) -> glib::Variant {
        let v = unsafe { self.end_unsafe() };
        std::mem::forget(self);
        v
    }
}

impl Drop for VariantBuilder {
    fn drop(&mut self) {
        unsafe { ffi::g_variant_builder_clear(self.as_ptr()) };
    }
}

trait VariantBuilderExt {
    fn as_ptr(&self) -> *mut ffi::GVariantBuilder;
    unsafe fn add<T: ToVariant>(&self, value: &T) {
        self.add(&value.to_variant());
    }
    unsafe fn add_value(&self, value: &glib::Variant) {
        ffi::g_variant_builder_add_value(self.as_ptr(), value.to_glib_none().0);
    }
    fn open(&self, ty: &VariantTy) -> VariantBuilderContainer<'_> {
        unsafe { ffi::g_variant_builder_open(self.as_ptr(), ty.to_glib_none().0) };
        VariantBuilderContainer {
            inner: NonNull::new(self.as_ptr()).unwrap(),
            phantom: PhantomData,
        }
    }
    unsafe fn end_unsafe(&self) -> glib::Variant {
        from_glib_none(ffi::g_variant_builder_end(self.as_ptr()))
    }
}

impl VariantBuilderExt for VariantBuilder {
    fn as_ptr(&self) -> *mut ffi::GVariantBuilder {
        &self.0 as *const _ as *mut _
    }
}

#[repr(transparent)]
struct VariantBuilderContainer<'t> {
    inner: NonNull<ffi::GVariantBuilder>,
    phantom: PhantomData<&'t ()>,
}

impl<'t> Drop for VariantBuilderContainer<'t> {
    fn drop(&mut self) {
        unsafe { ffi::g_variant_builder_close(self.inner.as_ptr()) };
    }
}

impl<'t> VariantBuilderExt for VariantBuilderContainer<'t> {
    fn as_ptr(&self) -> *mut ffi::GVariantBuilder {
        self.inner.as_ptr()
    }
}
