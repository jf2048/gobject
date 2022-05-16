macro_rules! declare_optional {
    ($ty:ty) => {
        pub mod optional {
            pub fn static_variant_type() -> std::borrow::Cow<'static, $crate::glib::VariantTy> {
                let mut builder = $crate::glib::GStringBuilder::new("m");
                builder.append(super::static_variant_type().as_str());
                std::borrow::Cow::Owned(
                    $crate::glib::VariantType::from_string(builder.into_string()).unwrap(),
                )
            }
            pub fn to_variant(value: &Option<$ty>) -> $crate::glib::Variant {
                match value.as_ref() {
                    Some(value) => $crate::glib::Variant::from_some(&super::to_variant(value)),
                    None => $crate::glib::Variant::from_none(&*super::static_variant_type()),
                }
            }
            pub fn from_variant(variant: &$crate::glib::Variant) -> Option<Option<$ty>> {
                if !variant.is_type(&*static_variant_type()) {
                    return None;
                }
                match variant.as_maybe() {
                    Some(variant) => Some(Some(super::from_variant(&variant)?)),
                    None => Some(None),
                }
            }
        }
    };
}

#[cfg(feature = "use_cairo")]
pub mod cairo;
#[cfg(feature = "use_gdk4")]
pub mod gdk4;
#[cfg(feature = "use_gio")]
pub mod gio;
pub mod glib;
#[cfg(feature = "use_graphene")]
pub mod graphene;
#[cfg(feature = "use_gtk4")]
pub mod gtk4;

use ::glib::ffi;
use ::glib::translate::{ToGlibPtr, ToGlibPtrMut};

struct VariantBuilder(ffi::GVariantBuilder);

#[doc(hidden)]
impl<'a> ToGlibPtrMut<'a, *mut ffi::GVariantBuilder> for VariantBuilder {
    type Storage = &'a mut Self;
    #[inline]
    fn to_glib_none_mut(
        &'a mut self,
    ) -> ::glib::translate::StashMut<'a, *mut ffi::GVariantBuilder, Self> {
        let ptr = &mut self.0 as *mut ffi::GVariantBuilder;
        ::glib::translate::StashMut(ptr, self)
    }
}

impl Drop for VariantBuilder {
    fn drop(&mut self) {
        unsafe {
            ffi::g_variant_builder_clear(self.to_glib_none_mut().0);
        }
    }
}

#[allow(dead_code)]
impl VariantBuilder {
    pub fn new(ty: &::glib::VariantTy) -> Self {
        unsafe {
            let mut builder = std::mem::MaybeUninit::uninit();
            ffi::g_variant_builder_init(builder.as_mut_ptr(), ty.to_glib_none().0);
            Self(builder.assume_init())
        }
    }
    pub unsafe fn add<T: ::glib::ToVariant>(&mut self, value: &T) {
        self.add_value(&value.to_variant());
    }
    pub unsafe fn add_value(&mut self, value: &::glib::Variant) {
        ffi::g_variant_builder_add_value(self.to_glib_none_mut().0, value.to_glib_none().0);
    }
    pub unsafe fn open<'b>(&'b mut self, ty: &::glib::VariantTy) -> VariantBuilderContainer<'b> {
        ffi::g_variant_builder_open(self.to_glib_none_mut().0, ty.to_glib_none().0);
        VariantBuilderContainer(self)
    }
    pub fn end(mut self) -> ::glib::Variant {
        let variant = unsafe {
            ::glib::translate::from_glib_full(ffi::g_variant_builder_end(self.to_glib_none_mut().0))
        };
        std::mem::forget(self);
        variant
    }
}

struct VariantBuilderContainer<'b>(&'b mut VariantBuilder);

#[allow(dead_code)]
impl<'b> VariantBuilderContainer<'b> {
    #[inline]
    pub unsafe fn add<T: ::glib::ToVariant>(&mut self, value: &T) {
        self.0.add(value);
    }
    #[inline]
    pub unsafe fn add_value(&mut self, value: &::glib::Variant) {
        self.0.add_value(value);
    }
    #[inline]
    pub unsafe fn open(&mut self, ty: &::glib::VariantTy) -> VariantBuilderContainer<'_> {
        self.0.open(ty)
    }
}

impl<'b> Drop for VariantBuilderContainer<'b> {
    fn drop(&mut self) {
        unsafe {
            ffi::g_variant_builder_close(self.0.to_glib_none_mut().0);
        }
    }
}

#[doc(hidden)]
pub trait ParentStaticVariantType {
    fn parent_static_variant_type() -> std::borrow::Cow<'static, ::glib::VariantTy>;
}

#[doc(hidden)]
pub trait ToParentVariant {
    fn to_parent_variant(&self) -> ::glib::Variant;
}

#[doc(hidden)]
pub trait FromParentVariant: Sized {
    fn from_parent_variant(variant: &::glib::Variant) -> Option<Self>;
    fn push_parent_values(variant: &::glib::Variant, args: &mut Vec<(&'static str, ::glib::Value)>);
}
