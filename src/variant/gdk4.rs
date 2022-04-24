use glib::{ToVariant, Variant, VariantTy};
use std::borrow::Cow;

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
