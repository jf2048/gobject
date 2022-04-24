use super::{VariantBuilder, VariantBuilderExt};
use glib::{ToVariant, Variant, VariantTy};
use std::borrow::Cow;

pub mod rectangle {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddd)") })
    }
    pub fn to_variant(r: &cairo::Rectangle) -> Variant {
        (r.x(), r.y(), r.width(), r.height()).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::Rectangle> {
        let (x, y, w, h) = variant.get()?;
        Some(cairo::Rectangle::new(x, y, w, h))
    }
    declare_optional!(cairo::Rectangle);
}

pub mod rectangle_int {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(iiii)") })
    }
    pub fn to_variant(r: &cairo::RectangleInt) -> Variant {
        (r.x(), r.y(), r.width(), r.height()).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::RectangleInt> {
        let (x, y, w, h) = variant.get()?;
        Some(cairo::RectangleInt::new(x, y, w, h))
    }
    declare_optional!(cairo::RectangleInt);
}

pub mod matrix {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddddd)") })
    }
    pub fn to_variant(m: &cairo::Matrix) -> Variant {
        (m.xx(), m.yx(), m.xy(), m.yy(), m.x0(), m.y0()).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::Matrix> {
        let (xx, yx, xy, yy, x0, y0) = variant.get()?;
        Some(cairo::Matrix::new(xx, yx, xy, yy, x0, y0))
    }
    declare_optional!(cairo::Matrix);
}

pub mod region {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("a(iiii)") })
    }
    pub fn to_variant(r: &cairo::Region) -> Variant {
        let builder = VariantBuilder::new(&static_variant_type());
        let count = r.num_rectangles();
        let count = count.max(0);
        for i in 0..count {
            let r = super::rectangle_int::to_variant(&r.rectangle(i));
            unsafe {
                builder.add_value(&r);
            }
        }
        builder.end()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::Region> {
        let rects = variant
            .iter()
            .filter_map(|v| super::rectangle_int::from_variant(&v))
            .collect::<Vec<_>>();
        Some(cairo::Region::create_rectangles(&rects))
    }
    declare_optional!(cairo::Region);
}

pub mod path {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("a(sv)") })
    }
    pub fn to_variant(p: &cairo::Path) -> Variant {
        let builder = VariantBuilder::new(&static_variant_type());
        for seg in p.iter() {
            let r = super::path_segment::to_variant(&seg);
            unsafe {
                builder.add_value(&r);
            }
        }
        builder.end()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::Path> {
        if !variant.is_type(&static_variant_type()) {
            return None;
        }
        let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).ok()?;
        let cr = cairo::Context::new(&surf).ok()?;
        for v in variant.iter() {
            let seg = super::path_segment::from_variant(&v)?;
            match seg {
                cairo::PathSegment::MoveTo((x, y)) => cr.move_to(x, y),
                cairo::PathSegment::LineTo((x, y)) => cr.line_to(x, y),
                cairo::PathSegment::CurveTo((ax, ay), (bx, by), (cx, cy)) => {
                    cr.curve_to(ax, ay, bx, by, cx, cy);
                }
                cairo::PathSegment::ClosePath => cr.close_path(),
            }
        }
        cr.copy_path().ok()
    }
    declare_optional!(cairo::Path);
}

pub mod path_segment {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(sv)") })
    }
    pub fn to_variant(seg: &cairo::PathSegment) -> Variant {
        match seg {
            cairo::PathSegment::MoveTo((x, y)) => ("m", (*x, *y).to_variant()).to_variant(),
            cairo::PathSegment::LineTo((x, y)) => ("l", (*x, *y).to_variant()).to_variant(),
            cairo::PathSegment::CurveTo((ax, ay), (bx, by), (cx, cy)) => {
                ("c", ((*ax, *ay), (*bx, *by), (*cx, *cy)).to_variant()).to_variant()
            }
            cairo::PathSegment::ClosePath => ("z", ().to_variant()).to_variant(),
        }
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::PathSegment> {
        if !variant.is_type(&static_variant_type()) {
            return None;
        }
        let name = variant.try_child_value(0)?;
        let name = name.str()?;
        let value = variant.try_child_value(1)?.as_variant()?;
        Some(match name {
            "m" => cairo::PathSegment::MoveTo(value.get()?),
            "l" => cairo::PathSegment::LineTo(value.get()?),
            "c" => {
                let (a, b, c) = value.get()?;
                cairo::PathSegment::CurveTo(a, b, c)
            }
            "z" => {
                value.get::<()>()?;
                cairo::PathSegment::ClosePath
            }
            _ => return None,
        })
    }
    declare_optional!(cairo::PathSegment);
}

pub mod pattern {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(sv)") })
    }
    pub fn to_variant(p: &cairo::Pattern) -> Variant {
        match p.type_() {
            cairo::PatternType::Solid => (
                "solid",
                super::solid_pattern::to_variant(
                    &cairo::SolidPattern::try_from(p.clone()).unwrap(),
                ),
            )
                .to_variant(),
            cairo::PatternType::LinearGradient => (
                "linear-gradient",
                super::linear_gradient::to_variant(
                    &cairo::LinearGradient::try_from(p.clone()).unwrap(),
                ),
            )
                .to_variant(),
            cairo::PatternType::RadialGradient => (
                "radial-gradient",
                super::radial_gradient::to_variant(
                    &cairo::RadialGradient::try_from(p.clone()).unwrap(),
                ),
            )
                .to_variant(),
            cairo::PatternType::Mesh => (
                "mesh",
                super::mesh::to_variant(&cairo::Mesh::try_from(p.clone()).unwrap()),
            )
                .to_variant(),
            t => panic!("unsupported pattern type {}", t),
        }
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::Pattern> {
        if !variant.is_type(&static_variant_type()) {
            return None;
        }
        let name = variant.try_child_value(0)?;
        let name = name.str()?;
        let value = variant.try_child_value(1)?.as_variant()?;
        match name {
            "solid" => {
                super::solid_pattern::from_variant(&value).map(|p| cairo::Pattern::clone(&p))
            }
            "linear-gradient" => {
                super::linear_gradient::from_variant(&value).map(|p| cairo::Pattern::clone(&p))
            }
            "radial-gradient" => {
                super::radial_gradient::from_variant(&value).map(|p| cairo::Pattern::clone(&p))
            }
            "mesh" => super::mesh::from_variant(&value).map(|p| cairo::Pattern::clone(&p)),
            _ => None,
        }
    }
    declare_optional!(cairo::Pattern);
}

pub mod solid_pattern {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddd)") })
    }
    pub fn to_variant(p: &cairo::SolidPattern) -> Variant {
        p.rgba().unwrap_or_default().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::SolidPattern> {
        let (r, g, b, a) = variant.get()?;
        Some(cairo::SolidPattern::from_rgba(r, g, b, a))
    }
    declare_optional!(cairo::SolidPattern);
}

pub mod gradient {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(sv)") })
    }
    pub fn to_variant(p: &cairo::Gradient) -> Variant {
        match p.type_() {
            cairo::PatternType::LinearGradient => (
                "linear-gradient",
                super::linear_gradient::to_variant(
                    &cairo::LinearGradient::try_from(p.clone()).unwrap(),
                ),
            )
                .to_variant(),
            cairo::PatternType::RadialGradient => (
                "radial-gradient",
                super::radial_gradient::to_variant(
                    &cairo::RadialGradient::try_from(p.clone()).unwrap(),
                ),
            )
                .to_variant(),
            _ => unreachable!(),
        }
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::Gradient> {
        if !variant.is_type(&static_variant_type()) {
            return None;
        }
        let name = variant.try_child_value(0)?;
        let name = name.str()?;
        let value = variant.try_child_value(1)?.as_variant()?;
        match name {
            "linear-gradient" => {
                super::linear_gradient::from_variant(&value).map(|p| cairo::Gradient::clone(&p))
            }
            "radial-gradient" => {
                super::radial_gradient::from_variant(&value).map(|p| cairo::Gradient::clone(&p))
            }
            _ => None,
        }
    }
    declare_optional!(cairo::Gradient);
}

pub mod linear_gradient {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe {
            VariantTy::from_str_unchecked("(ii((dddd)(dddd)(dddd)(dddd))(dd)(dd)a(d(dddd)))")
        })
    }
    pub fn to_variant(g: &cairo::LinearGradient) -> Variant {
        let builder = VariantBuilder::new(&static_variant_type());
        unsafe {
            builder.add(&(cairo::ffi::cairo_extend_t::from(g.extend()) as i32));
            builder.add(&(cairo::ffi::cairo_filter_t::from(g.filter()) as i32));
            builder.add_value(&super::matrix::to_variant(&g.matrix()));
            let (x0, y0, x1, y1) = g.linear_points().unwrap_or_default();
            builder.add(&(x0, y0));
            builder.add(&(x1, y1));
            let stops = builder.open(VariantTy::from_str_unchecked("a(d(dddd))"));
            let count = g.color_stop_count().unwrap_or_default();
            let count = count.max(0);
            for i in 0..count {
                let (o, r, g, b, a) = g.color_stop_rgba(i).unwrap_or_default();
                stops.add(&(o, (r, g, b, a)));
            }
        }
        builder.end()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::LinearGradient> {
        if !variant.is_type(&static_variant_type()) {
            return None;
        }
        let extend = variant.try_child_get::<i32>(0).ok()??;
        let extend = cairo::Extend::from(extend as cairo::ffi::cairo_extend_t);
        if matches!(extend, cairo::Extend::__Unknown(_)) {
            return None;
        }
        let filter = variant.try_child_get::<i32>(1).ok()??;
        let filter = cairo::Filter::from(filter as cairo::ffi::cairo_filter_t);
        if matches!(filter, cairo::Filter::__Unknown(_)) {
            return None;
        }
        let matrix = super::matrix::from_variant(&variant.try_child_value(2)?)?;
        let (x0, y0) = variant.try_child_get(3).ok()??;
        let (x1, y1) = variant.try_child_get(4).ok()??;
        let g = cairo::LinearGradient::new(x0, y0, x1, y1);
        g.set_extend(extend);
        g.set_filter(filter);
        g.set_matrix(matrix);
        for stop in variant.try_child_value(5)?.iter() {
            let (o, (r, g_, b, a)) = stop.get()?;
            g.add_color_stop_rgba(o, r, g_, b, a);
        }
        Some(g)
    }
    declare_optional!(cairo::LinearGradient);
}

pub mod radial_gradient {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe {
            VariantTy::from_str_unchecked("(ii((dddd)(dddd)(dddd)(dddd))((dd)d)((dd)d)a(d(dddd)))")
        })
    }
    pub fn to_variant(g: &cairo::RadialGradient) -> Variant {
        let builder = VariantBuilder::new(&static_variant_type());
        unsafe {
            builder.add(&(cairo::ffi::cairo_extend_t::from(g.extend()) as i32));
            builder.add(&(cairo::ffi::cairo_filter_t::from(g.filter()) as i32));
            builder.add_value(&super::matrix::to_variant(&g.matrix()));
            let (x0, y0, r0, x1, y1, r1) = g.radial_circles().unwrap_or_default();
            builder.add(&((x0, y0), r0));
            builder.add(&((x1, y1), r1));
            let stops = builder.open(VariantTy::from_str_unchecked("a(d(dddd))"));
            let count = g.color_stop_count().unwrap_or_default();
            let count = count.max(0);
            for i in 0..count {
                let (o, r, g, b, a) = g.color_stop_rgba(i).unwrap_or_default();
                stops.add(&(o, (r, g, b, a)));
            }
        }
        builder.end()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::RadialGradient> {
        if !variant.is_type(&static_variant_type()) {
            return None;
        }
        let extend = variant.try_child_get::<i32>(0).ok()??;
        let extend = cairo::Extend::from(extend as cairo::ffi::cairo_extend_t);
        if matches!(extend, cairo::Extend::__Unknown(_)) {
            return None;
        }
        let filter = variant.try_child_get::<i32>(1).ok()??;
        let filter = cairo::Filter::from(filter as cairo::ffi::cairo_filter_t);
        if matches!(filter, cairo::Filter::__Unknown(_)) {
            return None;
        }
        let matrix = super::matrix::from_variant(&variant.try_child_value(2)?)?;
        let ((x0, y0), r0) = variant.try_child_get(3).ok()??;
        let ((x1, y1), r1) = variant.try_child_get(4).ok()??;
        let g = cairo::RadialGradient::new(x0, y0, r0, x1, y1, r1);
        g.set_extend(extend);
        g.set_filter(filter);
        g.set_matrix(matrix);
        for stop in variant.try_child_value(5)?.iter() {
            let (o, (r, g_, b, a)) = stop.get()?;
            g.add_color_stop_rgba(o, r, g_, b, a);
        }
        Some(g)
    }
    declare_optional!(cairo::RadialGradient);
}

pub mod mesh {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe {
            VariantTy::from_str_unchecked(
                "(ii((dddd)(dddd)(dddd)(dddd))a(((dd)(dddd))((dd)(dddd))((dd)(dddd))((dd)(dddd))a(sv)))",
            )
        })
    }
    pub fn to_variant(m: &cairo::Mesh) -> Variant {
        let builder = VariantBuilder::new(&static_variant_type());
        unsafe {
            builder.add(&(cairo::ffi::cairo_extend_t::from(m.extend()) as i32));
            builder.add(&(cairo::ffi::cairo_filter_t::from(m.filter()) as i32));
            builder.add_value(&super::matrix::to_variant(&m.matrix()));
            let patches = builder.open(VariantTy::from_str_unchecked(
                "a(((dd)(dddd))((dd)(dddd))((dd)(dddd))((dd)(dddd))a(sv))",
            ));
            let count = m.patch_count().unwrap_or_default();
            for i in 0..count {
                let patch = patches.open(VariantTy::from_str_unchecked(
                    "(((dd)(dddd))((dd)(dddd))((dd)(dddd))((dd)(dddd))a(sv))",
                ));
                for corner in 0..4 {
                    let corner = corner.into();
                    let cp = m.control_point(i, corner).unwrap_or_default();
                    let color = m.corner_color_rgba(i, corner).unwrap_or_default();
                    patch.add(&(cp, color));
                }
                if let Ok(path) = m.path(i) {
                    patch.add_value(&super::path::to_variant(&path));
                } else {
                    patch.add_value(&Variant::array_from_iter_with_type::<Variant, _>(
                        &super::path_segment::static_variant_type(),
                        [],
                    ));
                }
            }
        }
        builder.end()
    }
    pub fn from_variant(variant: &Variant) -> Option<cairo::Mesh> {
        if !variant.is_type(&static_variant_type()) {
            return None;
        }
        let extend = variant.try_child_get::<i32>(0).ok()??;
        let extend = cairo::Extend::from(extend as cairo::ffi::cairo_extend_t);
        if matches!(extend, cairo::Extend::__Unknown(_)) {
            return None;
        }
        let filter = variant.try_child_get::<i32>(1).ok()??;
        let filter = cairo::Filter::from(filter as cairo::ffi::cairo_filter_t);
        if matches!(filter, cairo::Filter::__Unknown(_)) {
            return None;
        }
        let matrix = super::matrix::from_variant(&variant.try_child_value(2)?)?;
        let m = cairo::Mesh::new();
        m.set_extend(extend);
        m.set_filter(filter);
        m.set_matrix(matrix);
        for patch in variant.try_child_value(3)?.iter() {
            m.begin_patch();
            for corner in 0..4 {
                let ((x, y), (r, g, b, a)) = patch.try_child_get(corner as usize).ok()??;
                let corner = corner.into();
                m.set_control_point(corner, x, y);
                m.set_corner_color_rgba(corner, r, g, b, a);
            }
            for seg in variant.try_child_value(5)?.iter() {
                let seg = super::path_segment::from_variant(&seg)?;
                match seg {
                    cairo::PathSegment::MoveTo((x, y)) => m.move_to(x, y),
                    cairo::PathSegment::LineTo((x, y)) => m.line_to(x, y),
                    cairo::PathSegment::CurveTo((ax, ay), (bx, by), (cx, cy)) => {
                        m.curve_to(ax, ay, bx, by, cx, cy);
                    }
                    _ => {}
                }
            }
            m.end_patch();
        }
        Some(m)
    }
    declare_optional!(cairo::Mesh);
}
