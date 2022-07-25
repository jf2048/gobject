use crate::{OnceBool, OnceBox, OnceCell, SyncOnceCell};

pub trait ParamSpecBuildable {
    type ParamSpec;
}

impl ParamSpecBuildable for bool {
    type ParamSpec = glib::ParamSpecBoolean;
}
impl ParamSpecBuildable for i8 {
    type ParamSpec = glib::ParamSpecChar;
}
impl ParamSpecBuildable for i32 {
    type ParamSpec = glib::ParamSpecInt;
}
impl ParamSpecBuildable for i64 {
    type ParamSpec = glib::ParamSpecInt64;
}
impl ParamSpecBuildable for glib::ILong {
    type ParamSpec = glib::ParamSpecLong;
}
impl ParamSpecBuildable for u8 {
    type ParamSpec = glib::ParamSpecUChar;
}
impl ParamSpecBuildable for u32 {
    type ParamSpec = glib::ParamSpecUInt;
}
impl ParamSpecBuildable for u64 {
    type ParamSpec = glib::ParamSpecUInt64;
}
impl ParamSpecBuildable for glib::ULong {
    type ParamSpec = glib::ParamSpecULong;
}
impl ParamSpecBuildable for glib::UChar {
    type ParamSpec = <u8 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for f32 {
    type ParamSpec = glib::ParamSpecFloat;
}
impl ParamSpecBuildable for f64 {
    type ParamSpec = glib::ParamSpecDouble;
}
impl ParamSpecBuildable for char {
    type ParamSpec = glib::ParamSpecUnichar;
}
impl ParamSpecBuildable for String {
    type ParamSpec = glib::ParamSpecString;
}
impl ParamSpecBuildable for glib::GString {
    type ParamSpec = glib::ParamSpecString;
}
impl ParamSpecBuildable for glib::Type {
    type ParamSpec = glib::ParamSpecGType;
}
impl<T> ParamSpecBuildable for *mut T {
    type ParamSpec = glib::ParamSpecPointer;
}
impl<T> ParamSpecBuildable for *const T {
    type ParamSpec = glib::ParamSpecPointer;
}
impl<T> ParamSpecBuildable for std::ptr::NonNull<T> {
    type ParamSpec = glib::ParamSpecPointer;
}
impl ParamSpecBuildable for std::num::NonZeroI8 {
    type ParamSpec = <i8 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::num::NonZeroI32 {
    type ParamSpec = <i32 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::num::NonZeroI64 {
    type ParamSpec = <i64 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::num::NonZeroU8 {
    type ParamSpec = <u8 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::num::NonZeroU32 {
    type ParamSpec = <u32 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::num::NonZeroU64 {
    type ParamSpec = <u64 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for glib::ParamSpec {
    type ParamSpec = glib::ParamSpecParam;
}
impl ParamSpecBuildable for glib::Variant {
    type ParamSpec = glib::ParamSpecVariant;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for Option<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for std::marker::PhantomData<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for std::cell::Cell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for std::cell::RefCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for std::sync::Mutex<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for std::sync::RwLock<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for OnceCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for SyncOnceCell<T> {
    type ParamSpec = T::ParamSpec;
}
impl<T: ParamSpecBuildable> ParamSpecBuildable for OnceBox<T> {
    type ParamSpec = T::ParamSpec;
}
impl ParamSpecBuildable for std::sync::atomic::AtomicBool {
    type ParamSpec = <bool as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::sync::atomic::AtomicI8 {
    type ParamSpec = <i8 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::sync::atomic::AtomicI32 {
    type ParamSpec = <i32 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::sync::atomic::AtomicI64 {
    type ParamSpec = <i64 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::sync::atomic::AtomicU8 {
    type ParamSpec = <u8 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::sync::atomic::AtomicU32 {
    type ParamSpec = <u32 as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for std::sync::atomic::AtomicU64 {
    type ParamSpec = <u64 as ParamSpecBuildable>::ParamSpec;
}
impl<T> ParamSpecBuildable for std::sync::atomic::AtomicPtr<T> {
    type ParamSpec = <glib::types::Pointer as ParamSpecBuildable>::ParamSpec;
}
impl ParamSpecBuildable for OnceBool {
    type ParamSpec = <bool as ParamSpecBuildable>::ParamSpec;
}
impl<T: ParamSpecBuildable + glib::ObjectType> ParamSpecBuildable for glib::WeakRef<T> {
    type ParamSpec = T::ParamSpec;
}
#[cfg(feature = "use_gtk4")]
impl<T> ParamSpecBuildable for gtk4::TemplateChild<T>
where
    T: ParamSpecBuildable
        + glib::ObjectType
        + glib::translate::FromGlibPtrNone<*mut <T as glib::ObjectType>::GlibType>,
{
    type ParamSpec = T::ParamSpec;
}

#[cfg(feature = "use_gst")]
impl ParamSpecBuildable for gst::Array {
    type ParamSpec = gst::ParamSpecArray;
}

#[cfg(feature = "use_gst")]
impl ParamSpecBuildable for gst::Fraction {
    type ParamSpec = gst::ParamSpecFraction;
}
