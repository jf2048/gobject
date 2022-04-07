use serde::ser::SerializeSeq;
use serde::ser::SerializeTuple;
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

pub mod rectangle {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "cairo::Rectangle")]
    struct Rectangle(f64, f64, f64, f64);
    pub fn serialize<S: Serializer>(r: &cairo::Rectangle, s: S) -> Result<S::Ok, S::Error> {
        Rectangle(r.x(), r.y(), r.width(), r.height()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Rectangle, D::Error> {
        let Rectangle(x, y, w, h) = Rectangle::deserialize(d)?;
        Ok(cairo::Rectangle::new(x, y, w, h))
    }
}

pub mod rectangle_int {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "cairo::RectangleInt")]
    struct RectangleInt(i32, i32, i32, i32);
    pub fn serialize<S: Serializer>(r: &cairo::RectangleInt, s: S) -> Result<S::Ok, S::Error> {
        RectangleInt(r.x(), r.y(), r.width(), r.height()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::RectangleInt, D::Error> {
        let RectangleInt(x, y, w, h) = RectangleInt::deserialize(d)?;
        Ok(cairo::RectangleInt::new(x, y, w, h))
    }
}

pub mod matrix {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "cairo::Matrix")]
    pub(super) struct Matrix(f64, f64, f64, f64, f64, f64);
    impl From<cairo::Matrix> for Matrix {
        fn from(m: cairo::Matrix) -> Self {
            Self(m.xx(), m.yx(), m.xy(), m.yy(), m.x0(), m.y0())
        }
    }
    impl From<Matrix> for cairo::Matrix {
        fn from(m: Matrix) -> Self {
            let Matrix(xx, yx, xy, yy, x0, y0) = m;
            cairo::Matrix::new(xx, yx, xy, yy, x0, y0)
        }
    }
    pub fn serialize<S: Serializer>(m: &cairo::Matrix, s: S) -> Result<S::Ok, S::Error> {
        Matrix::from(*m).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Matrix, D::Error> {
        Ok(Matrix::deserialize(d)?.into())
    }
}

pub mod region {
    use super::*;
    struct Rects<'r>(&'r cairo::Region);
    impl<'r> Serialize for Rects<'r> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let count = self.0.num_rectangles();
            let mut seq = s.serialize_seq(Some(count as usize))?;
            for i in 0..count {
                let r = self.0.rectangle(i);
                seq.serialize_element(&(r.x(), r.y(), r.width(), r.height()))?;
            }
            seq.end()
        }
    }
    pub fn serialize<S: Serializer>(r: &cairo::Region, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_newtype_struct("cairo::Region", &Rects(r))
    }
    #[derive(Deserialize)]
    #[serde(rename = "cairo::Region")]
    struct Region(Vec<(i32, i32, i32, i32)>);
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Region, D::Error> {
        let Region(rects) = Region::deserialize(d)?;
        let rects = rects
            .into_iter()
            .map(|(x, y, w, h)| cairo::RectangleInt::new(x, y, w, h))
            .collect::<Vec<_>>();
        Ok(cairo::Region::create_rectangles(&rects))
    }
}

pub mod path {
    use super::*;
    pub(super) struct Path<'p>(pub(super) &'p cairo::Path);
    impl<'p> Serialize for Path<'p> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut seq = s.serialize_seq(None)?;
            for seg in self.0.iter() {
                seq.serialize_element(&path_segment::PathSegment::from(seg))?;
            }
            seq.end()
        }
    }
    pub fn serialize<S: Serializer>(p: &cairo::Path, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_newtype_struct("cairo::Path", &Path(p))
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Path, D::Error> {
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = cairo::Path;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a sequence")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let surf = cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1)
                    .map_err(de::Error::custom)?;
                let cr = cairo::Context::new(&surf).map_err(de::Error::custom)?;
                while let Some(seg) = seq.next_element::<path_segment::PathSegment>()? {
                    use path_segment::PathSegment::*;
                    match seg {
                        M((x, y)) => cr.move_to(x, y),
                        L((x, y)) => cr.line_to(x, y),
                        C((x1, y1), (x2, y2), (x3, y3)) => cr.curve_to(x1, y1, x2, y2, x3, y3),
                        Z => cr.close_path(),
                    }
                }
                Ok(cr.copy_path().map_err(de::Error::custom)?)
            }
        }
        d.deserialize_newtype_struct("cairo::Path", Visitor)
    }
}

pub mod path_segment {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "cairo::PathSegment")]
    pub(super) enum PathSegment {
        M((f64, f64)),
        L((f64, f64)),
        C((f64, f64), (f64, f64), (f64, f64)),
        Z,
    }
    impl From<cairo::PathSegment> for PathSegment {
        fn from(seg: cairo::PathSegment) -> Self {
            match seg {
                cairo::PathSegment::MoveTo(p) => PathSegment::M(p),
                cairo::PathSegment::LineTo(p) => PathSegment::L(p),
                cairo::PathSegment::CurveTo(a, b, c) => PathSegment::C(a, b, c),
                cairo::PathSegment::ClosePath => PathSegment::Z,
            }
        }
    }
    impl From<PathSegment> for cairo::PathSegment {
        fn from(seg: PathSegment) -> Self {
            match seg {
                PathSegment::M(p) => cairo::PathSegment::MoveTo(p),
                PathSegment::L(p) => cairo::PathSegment::LineTo(p),
                PathSegment::C(a, b, c) => cairo::PathSegment::CurveTo(a, b, c),
                PathSegment::Z => cairo::PathSegment::ClosePath,
            }
        }
    }
    pub fn serialize<S: Serializer>(seg: &cairo::PathSegment, s: S) -> Result<S::Ok, S::Error> {
        PathSegment::from(*seg).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::PathSegment, D::Error> {
        Ok(PathSegment::deserialize(d)?.into())
    }
}

pub mod pattern {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "cairo::Pattern")]
    #[allow(dead_code)]
    pub(super) enum Pattern {
        #[serde(with = "solid_pattern_inner")]
        Solid(cairo::SolidPattern),
        #[serde(skip)]
        Surface,
        #[serde(with = "linear_gradient_inner")]
        LinearGradient(cairo::LinearGradient),
        #[serde(with = "radial_gradient_inner")]
        RadialGradient(cairo::RadialGradient),
        #[serde(with = "mesh_inner")]
        Mesh(cairo::Mesh),
    }
    pub fn serialize<S: Serializer>(pat: &cairo::Pattern, s: S) -> Result<S::Ok, S::Error> {
        match pat.type_() {
            cairo::PatternType::Solid => {
                Pattern::Solid(cairo::SolidPattern::try_from(pat.clone()).unwrap()).serialize(s)
            }
            cairo::PatternType::LinearGradient => {
                Pattern::LinearGradient(cairo::LinearGradient::try_from(pat.clone()).unwrap())
                    .serialize(s)
            }
            cairo::PatternType::RadialGradient => {
                Pattern::RadialGradient(cairo::RadialGradient::try_from(pat.clone()).unwrap())
                    .serialize(s)
            }
            cairo::PatternType::Mesh => {
                Pattern::Mesh(cairo::Mesh::try_from(pat.clone()).unwrap()).serialize(s)
            }
            t => Err(ser::Error::custom(format!(
                "unsupported pattern type {}",
                t
            ))),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Pattern, D::Error> {
        Ok(match Pattern::deserialize(d)? {
            Pattern::Solid(p) => cairo::Pattern::clone(&p),
            Pattern::LinearGradient(p) => cairo::Pattern::clone(&p),
            Pattern::RadialGradient(p) => cairo::Pattern::clone(&p),
            Pattern::Mesh(p) => cairo::Pattern::clone(&p),
            _ => unreachable!(),
        })
    }
}

pub mod solid_pattern {
    use super::*;
    pub fn serialize<S: Serializer>(pat: &cairo::SolidPattern, s: S) -> Result<S::Ok, S::Error> {
        pattern::Pattern::Solid(pat.clone()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::SolidPattern, D::Error> {
        match pattern::Pattern::deserialize(d)? {
            pattern::Pattern::Solid(p) => Ok(p),
            _ => Err(de::Error::custom("wrong pattern type")),
        }
    }
}

pub mod gradient {
    use super::*;
    pub fn serialize<S: Serializer>(pat: &cairo::Gradient, s: S) -> Result<S::Ok, S::Error> {
        match pat.type_() {
            cairo::PatternType::LinearGradient => pattern::Pattern::LinearGradient(
                cairo::LinearGradient::try_from(pat.clone()).unwrap(),
            )
            .serialize(s),
            cairo::PatternType::RadialGradient => pattern::Pattern::RadialGradient(
                cairo::RadialGradient::try_from(pat.clone()).unwrap(),
            )
            .serialize(s),
            _ => unreachable!(),
        }
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Gradient, D::Error> {
        Ok(match pattern::Pattern::deserialize(d)? {
            pattern::Pattern::LinearGradient(p) => cairo::Gradient::clone(&p),
            pattern::Pattern::RadialGradient(p) => cairo::Gradient::clone(&p),
            _ => unreachable!(),
        })
    }
}

pub mod linear_gradient {
    use super::*;
    pub fn serialize<S: Serializer>(pat: &cairo::LinearGradient, s: S) -> Result<S::Ok, S::Error> {
        pattern::Pattern::LinearGradient(pat.clone()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::LinearGradient, D::Error> {
        match pattern::Pattern::deserialize(d)? {
            pattern::Pattern::LinearGradient(p) => Ok(p),
            _ => Err(de::Error::custom("wrong pattern type")),
        }
    }
}

pub mod radial_gradient {
    use super::*;
    pub fn serialize<S: Serializer>(pat: &cairo::RadialGradient, s: S) -> Result<S::Ok, S::Error> {
        pattern::Pattern::RadialGradient(pat.clone()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::RadialGradient, D::Error> {
        match pattern::Pattern::deserialize(d)? {
            pattern::Pattern::RadialGradient(p) => Ok(p),
            _ => Err(de::Error::custom("wrong pattern type")),
        }
    }
}

pub mod mesh {
    use super::*;
    pub fn serialize<S: Serializer>(pat: &cairo::Mesh, s: S) -> Result<S::Ok, S::Error> {
        pattern::Pattern::Mesh(pat.clone()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Mesh, D::Error> {
        match pattern::Pattern::deserialize(d)? {
            pattern::Pattern::Mesh(p) => Ok(p),
            _ => Err(de::Error::custom("wrong pattern type")),
        }
    }
}

macro_rules! wrap_enum {
    ($name:ident, $wrapped:path, $sername:literal, $($variants:ident),+ $(,)?) => {
        #[derive(Serialize, Deserialize)]
        #[serde(rename = $sername)]
        enum $name {
            $($variants),+
        }
        impl TryFrom<$wrapped> for $name {
            type Error = glib::BoolError;
            fn try_from(e: $wrapped) -> Result<Self, Self::Error> {
                Ok(match e {
                    $(<$wrapped>::$variants => $name::$variants,)+
                    _ => return Err(glib::bool_error!(concat!("Unknown ", stringify!($name)))),
                })
            }
        }
        impl From<$name> for $wrapped {
            fn from(e: $name) -> Self {
                match e {
                    $($name::$variants => <$wrapped>::$variants,)+
                }
            }
        }
    };
}

wrap_enum!(
    PatternFilter,
    cairo::Filter,
    "cairo::Filter",
    Fast,
    Good,
    Best,
    Nearest,
    Bilinear,
    Gaussian,
);

wrap_enum!(
    PatternExtend,
    cairo::Extend,
    "cairo::Extend",
    None,
    Repeat,
    Reflect,
    Pad,
);

mod solid_pattern_inner {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "cairo::SolidPattern")]
    struct SolidPattern(f64, f64, f64, f64);
    pub(super) fn serialize<S: Serializer>(
        p: &cairo::SolidPattern,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let (r, g, b, a) = p.rgba().map_err(ser::Error::custom)?;
        SolidPattern(r, g, b, a).serialize(s)
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<cairo::SolidPattern, D::Error> {
        let SolidPattern(r, g, b, a) = SolidPattern::deserialize(d)?;
        Ok(cairo::SolidPattern::from_rgba(r, g, b, a))
    }
}

mod linear_gradient_inner {
    use super::*;
    #[derive(Serialize)]
    #[serde(rename = "cairo::LinearGradient")]
    struct Linear<'p>(
        PatternExtend,
        PatternFilter,
        matrix::Matrix,
        Points<'p>,
        Stops<'p>,
    );
    struct Points<'p>(&'p cairo::LinearGradient);
    struct Stops<'p>(&'p cairo::LinearGradient);
    impl<'p> Serialize for Points<'p> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.0
                .linear_points()
                .map_err(ser::Error::custom)?
                .serialize(s)
        }
    }
    impl<'p> Serialize for Stops<'p> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let count = self.0.color_stop_count().map_err(ser::Error::custom)?;
            let count = count.max(0);
            let mut seq = s.serialize_seq(Some(count as usize))?;
            for i in 0..count {
                let stop = self.0.color_stop_rgba(i).map_err(ser::Error::custom)?;
                seq.serialize_element(&stop)?;
            }
            seq.end()
        }
    }
    pub(super) fn serialize<S: Serializer>(
        l: &cairo::LinearGradient,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let linear = Linear(
            l.extend().try_into().map_err(ser::Error::custom)?,
            l.filter().try_into().map_err(ser::Error::custom)?,
            l.matrix().into(),
            Points(l),
            Stops(l),
        );
        s.serialize_newtype_struct("cairo::LinearGradient", &linear)
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<cairo::LinearGradient, D::Error> {
        struct StopVisitor<'p>(&'p cairo::LinearGradient);
        impl<'de, 'p> de::Visitor<'de> for StopVisitor<'p> {
            type Value = ();
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a sequence")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                while let Some((o, r, g, b, a)) = seq.next_element()? {
                    self.0.add_color_stop_rgba(o, r, g, b, a);
                }
                Ok(())
            }
        }
        impl<'de, 'p> serde::de::DeserializeSeed<'de> for StopVisitor<'p> {
            type Value = ();
            fn deserialize<D>(self, d: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                d.deserialize_seq(self)
            }
        }
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = cairo::LinearGradient;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("tuple struct with length 5")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let extend = seq
                    .next_element::<PatternExtend>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"tuple struct with length 5"))?;
                let filter = seq
                    .next_element::<PatternFilter>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"tuple struct with length 5"))?;
                let matrix = seq
                    .next_element::<matrix::Matrix>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"tuple struct with length 5"))?;
                let (x0, y0, x1, y1) = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"tuple struct with length 5"))?;
                let linear = cairo::LinearGradient::new(x0, y0, x1, y1);
                linear.set_extend(extend.into());
                linear.set_filter(filter.into());
                linear.set_matrix(matrix.into());
                seq.next_element_seed(StopVisitor(&linear))?
                    .ok_or_else(|| de::Error::invalid_length(4, &"tuple struct with length 5"))?;
                Ok(linear)
            }
        }
        d.deserialize_tuple_struct("cairo::LinearGradient", 5, Visitor)
    }
}

mod radial_gradient_inner {
    use super::*;
    #[derive(Serialize)]
    #[serde(rename = "cairo::RadialGradient")]
    struct Radial<'p>(
        PatternExtend,
        PatternFilter,
        matrix::Matrix,
        Points<'p>,
        Stops<'p>,
    );
    struct Points<'p>(&'p cairo::RadialGradient);
    struct Stops<'p>(&'p cairo::RadialGradient);
    impl<'p> Serialize for Points<'p> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            self.0
                .radial_circles()
                .map_err(ser::Error::custom)?
                .serialize(s)
        }
    }
    impl<'p> Serialize for Stops<'p> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let count = self.0.color_stop_count().map_err(ser::Error::custom)?;
            let count = count.max(0);
            let mut seq = s.serialize_seq(Some(count as usize))?;
            for i in 0..count {
                let stop = self.0.color_stop_rgba(i).map_err(ser::Error::custom)?;
                seq.serialize_element(&stop)?;
            }
            seq.end()
        }
    }
    pub(super) fn serialize<S: Serializer>(
        r: &cairo::RadialGradient,
        s: S,
    ) -> Result<S::Ok, S::Error> {
        let radial = Radial(
            r.extend().try_into().map_err(ser::Error::custom)?,
            r.filter().try_into().map_err(ser::Error::custom)?,
            r.matrix().into(),
            Points(r),
            Stops(r),
        );
        s.serialize_newtype_struct("cairo::RadialGradient", &radial)
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        d: D,
    ) -> Result<cairo::RadialGradient, D::Error> {
        struct StopVisitor<'p>(&'p cairo::RadialGradient);
        impl<'de, 'p> de::Visitor<'de> for StopVisitor<'p> {
            type Value = ();
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a sequence")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                while let Some((o, r, g, b, a)) = seq.next_element()? {
                    self.0.add_color_stop_rgba(o, r, g, b, a);
                }
                Ok(())
            }
        }
        impl<'de, 'p> serde::de::DeserializeSeed<'de> for StopVisitor<'p> {
            type Value = ();
            fn deserialize<D>(self, d: D) -> Result<Self::Value, D::Error>
            where
                D: Deserializer<'de>,
            {
                d.deserialize_seq(self)
            }
        }
        struct Visitor;
        impl<'de> de::Visitor<'de> for Visitor {
            type Value = cairo::RadialGradient;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("tuple struct with length 5")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let extend = seq
                    .next_element::<PatternExtend>()?
                    .ok_or_else(|| de::Error::invalid_length(0, &"tuple struct with length 5"))?;
                let filter = seq
                    .next_element::<PatternFilter>()?
                    .ok_or_else(|| de::Error::invalid_length(1, &"tuple struct with length 5"))?;
                let matrix = seq
                    .next_element::<matrix::Matrix>()?
                    .ok_or_else(|| de::Error::invalid_length(2, &"tuple struct with length 5"))?;
                let (x0, y0, r0, x1, y1, r1) = seq
                    .next_element()?
                    .ok_or_else(|| de::Error::invalid_length(3, &"tuple struct with length 5"))?;
                let radial = cairo::RadialGradient::new(x0, y0, r0, x1, y1, r1);
                radial.set_extend(extend.into());
                radial.set_filter(filter.into());
                radial.set_matrix(matrix.into());
                seq.next_element_seed(StopVisitor(&radial))?
                    .ok_or_else(|| de::Error::invalid_length(4, &"tuple struct with length 5"))?;
                Ok(radial)
            }
        }
        d.deserialize_tuple_struct("cairo::RadialGradient", 5, Visitor)
    }
}

mod mesh_inner {
    use super::*;
    #[derive(Serialize)]
    #[serde(rename = "cairo::Mesh")]
    struct Mesh<'p>(
        PatternExtend,
        PatternFilter,
        matrix::Matrix,
        MeshPatches<'p>,
    );
    struct MeshPatches<'p>(&'p cairo::Mesh);
    struct MeshPatch<'p>(&'p cairo::Mesh, usize);
    impl<'p> Serialize for MeshPatch<'p> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let mut seq = s.serialize_tuple(9)?;
            for corner in 0..4 {
                let corner = corner.into();
                let cp = self
                    .0
                    .control_point(self.1, corner)
                    .map_err(ser::Error::custom)?;
                seq.serialize_element(&cp)?;
                let color = self
                    .0
                    .corner_color_rgba(self.1, corner)
                    .map_err(ser::Error::custom)?;
                seq.serialize_element(&color)?;
            }
            let path = self.0.path(self.1).map_err(ser::Error::custom)?;
            seq.serialize_element(&path::Path(&path))?;
            seq.end()
        }
    }
    impl<'p> Serialize for MeshPatches<'p> {
        fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
            let count = self.0.patch_count().map_err(ser::Error::custom)?;
            let mut seq = s.serialize_seq(Some(count))?;
            for i in 0..count {
                seq.serialize_element(&MeshPatch(self.0, i))?;
            }
            seq.end()
        }
    }
    pub(super) fn serialize<S: Serializer>(m: &cairo::Mesh, s: S) -> Result<S::Ok, S::Error> {
        let mesh = Mesh(
            m.extend().try_into().map_err(ser::Error::custom)?,
            m.filter().try_into().map_err(ser::Error::custom)?,
            m.matrix().into(),
            MeshPatches(m),
        );
        s.serialize_newtype_struct("cairo::Mesh", &mesh)
    }
    #[derive(Deserialize)]
    #[serde(rename = "cairo::Mesh")]
    struct OwnedMesh(PatternExtend, PatternFilter, matrix::Matrix, OwnedPatches);
    struct OwnedPatches(cairo::Mesh);
    impl<'de> Deserialize<'de> for OwnedPatches {
        fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            struct PathVisitor<'m>(&'m cairo::Mesh);
            impl<'de, 'm> de::Visitor<'de> for PathVisitor<'m> {
                type Value = ();
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a sequence")
                }
                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: de::SeqAccess<'de>,
                {
                    use path_segment::PathSegment::*;
                    while let Some(seg) = seq.next_element::<path_segment::PathSegment>()? {
                        match seg {
                            M((x, y)) => self.0.move_to(x, y),
                            L((x, y)) => self.0.line_to(x, y),
                            C((x1, y1), (x2, y2), (x3, y3)) => {
                                self.0.curve_to(x1, y1, x2, y2, x3, y3)
                            }
                            _ => {}
                        }
                    }
                    Ok(())
                }
            }
            impl<'de, 'm> serde::de::DeserializeSeed<'de> for PathVisitor<'m> {
                type Value = ();
                fn deserialize<D>(self, d: D) -> Result<Self::Value, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    d.deserialize_seq(self)
                }
            }
            struct PatchVisitor<'m>(&'m cairo::Mesh);
            impl<'de, 'm> de::Visitor<'de> for PatchVisitor<'m> {
                type Value = ();
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a patch tuple")
                }
                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: de::SeqAccess<'de>,
                {
                    self.0.begin_patch();
                    for corner in 0..4 {
                        let (x, y) = seq.next_element()?.ok_or_else(|| {
                            de::Error::invalid_length(
                                2 * corner as usize,
                                &"tuple struct with length 9",
                            )
                        })?;
                        let (r, g, b, a) = seq.next_element()?.ok_or_else(|| {
                            de::Error::invalid_length(
                                1 + 2 * corner as usize,
                                &"tuple struct with length 9",
                            )
                        })?;
                        let corner = corner.into();
                        self.0.set_control_point(corner, x, y);
                        self.0.set_corner_color_rgba(corner, r, g, b, a);
                    }
                    seq.next_element_seed(PathVisitor(self.0))?.ok_or_else(|| {
                        de::Error::invalid_length(8, &"tuple struct with length 9")
                    })?;
                    self.0.end_patch();
                    Ok(())
                }
            }
            impl<'de, 'm> serde::de::DeserializeSeed<'de> for PatchVisitor<'m> {
                type Value = ();
                fn deserialize<D>(self, d: D) -> Result<Self::Value, D::Error>
                where
                    D: Deserializer<'de>,
                {
                    d.deserialize_seq(self)
                }
            }
            struct Visitor;
            impl<'de> de::Visitor<'de> for Visitor {
                type Value = cairo::Mesh;
                fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                    f.write_str("a sequence")
                }
                fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
                where
                    A: de::SeqAccess<'de>,
                {
                    let mesh = cairo::Mesh::new();
                    while seq.next_element_seed(PatchVisitor(&mesh))?.is_some() {}
                    Ok(mesh)
                }
            }
            Ok(Self(d.deserialize_seq(Visitor)?))
        }
    }
    pub(super) fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<cairo::Mesh, D::Error> {
        let mesh = OwnedMesh::deserialize(d)?;
        let m = mesh.3 .0;
        m.set_extend(mesh.0.into());
        m.set_filter(mesh.1.into());
        m.set_matrix(mesh.2.into());
        Ok(m)
    }
}
