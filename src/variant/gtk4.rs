use super::{VariantBuilder, VariantBuilderExt};
use glib::{ToVariant, Variant, VariantTy};
use gtk4::prelude::*;
use std::borrow::Cow;

pub mod adjustment {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddddd)") })
    }
    pub fn to_variant(a: &gtk4::Adjustment) -> Variant {
        (
            a.value(),
            a.lower(),
            a.upper(),
            a.step_increment(),
            a.page_increment(),
            a.page_size(),
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gtk4::Adjustment> {
        let (v, l, u, si, pi, ps) = variant.get()?;
        Some(gtk4::Adjustment::new(v, l, u, si, pi, ps))
    }
    declare_optional!(gtk4::Adjustment);
}

pub mod border {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(nnnn)") })
    }
    pub fn to_variant(b: &gtk4::Border) -> Variant {
        (b.left(), b.right(), b.top(), b.bottom()).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gtk4::Border> {
        let (l, r, t, b) = variant.get()?;
        Some(
            gtk4::Border::builder()
                .left(l)
                .right(r)
                .top(t)
                .bottom(b)
                .build(),
        )
    }
    declare_optional!(gtk4::Border);
}

pub mod paper_size {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(ps: &gtk4::PaperSize) -> Variant {
        let kf = glib::KeyFile::new();
        ps.clone().to_key_file(&kf, "paper_size");
        kf.to_data().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gtk4::PaperSize> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        let d = variant.str()?;
        let kf = glib::KeyFile::new();
        glib::KeyFile::load_from_data(&kf, d, glib::KeyFileFlags::NONE).ok()?;
        gtk4::PaperSize::from_key_file(&kf, Some("paper_size")).ok()
    }
    declare_optional!(gtk4::PaperSize);
}

pub mod string_object {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING)
    }
    pub fn to_variant(so: &gtk4::StringObject) -> Variant {
        so.string().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gtk4::StringObject> {
        if !variant.is_type(VariantTy::STRING) {
            return None;
        }
        Some(gtk4::StringObject::new(variant.str()?))
    }
    declare_optional!(gtk4::StringObject);
}

pub mod string_list {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(VariantTy::STRING_ARRAY)
    }
    pub fn to_variant(sl: &gtk4::StringList) -> Variant {
        let builder = VariantBuilder::new(VariantTy::STRING_ARRAY);
        let count = sl.n_items();
        for i in 0..count {
            if let Some(s) = sl.string(i) {
                unsafe {
                    builder.add(&s.as_str());
                }
            }
        }
        builder.end()
    }
    pub fn from_variant(variant: &Variant) -> Option<gtk4::StringList> {
        if !variant.is_type(VariantTy::STRING_ARRAY) {
            return None;
        }
        let sl = gtk4::StringList::new(&[]);
        for variant in variant.iter() {
            if let Some(s) = variant.str() {
                sl.append(s);
            }
        }
        Some(sl)
    }
    declare_optional!(gtk4::StringList);
}
