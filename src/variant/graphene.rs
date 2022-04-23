use glib::{ToVariant, Variant, VariantTy};
use std::borrow::Cow;

pub mod box_ {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("((ddd)(ddd))") })
    }
    pub fn to_variant(b: &graphene::Box) -> Variant {
        let a = b.min();
        let b = b.max();
        (
            (a.x() as f64, a.y() as f64, a.z() as f64),
            (b.x() as f64, b.y() as f64, b.z() as f64),
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Box> {
        let ((ax, ay, az), (bx, by, bz)) = variant.get::<((f64, f64, f64), (f64, f64, f64))>()?;
        let a = graphene::Point3D::new(ax as f32, ay as f32, az as f32);
        let b = graphene::Point3D::new(bx as f32, by as f32, bz as f32);
        Some(graphene::Box::new(Some(&a), Some(&b)))
    }
}

pub mod euler {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(ddd)") })
    }
    pub fn to_variant(e: &graphene::Euler) -> Variant {
        (e.x() as f64, e.y() as f64, e.z() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Euler> {
        let (x, y, z) = variant.get::<(f64, f64, f64)>()?;
        Some(graphene::Euler::new(x as f32, y as f32, z as f32))
    }
}

pub mod frustum {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe {
            VariantTy::from_str_unchecked("(((ddd)d)((ddd)d)((ddd)d)((ddd)d)((ddd)d)((ddd)d))")
        })
    }
    pub fn to_variant(f: &graphene::Frustum) -> Variant {
        let p = f.planes();
        #[inline]
        fn to_tuple(p: &graphene::Plane) -> ((f64, f64, f64), f64) {
            let n = p.normal();
            (
                (n.x() as f64, n.y() as f64, n.z() as f64),
                p.constant() as f64,
            )
        }
        (
            to_tuple(&p[0]),
            to_tuple(&p[1]),
            to_tuple(&p[2]),
            to_tuple(&p[3]),
            to_tuple(&p[4]),
            to_tuple(&p[5]),
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Frustum> {
        let (p0, p1, p2, p3, p4, p5) = variant.get()?;
        #[inline]
        fn to_plane(((x, y, z), c): ((f64, f64, f64), f64)) -> graphene::Plane {
            graphene::Plane::new(
                Some(&graphene::Vec3::new(x as f32, y as f32, z as f32)),
                c as f32,
            )
        }
        let p0 = to_plane(p0);
        let p1 = to_plane(p1);
        let p2 = to_plane(p2);
        let p3 = to_plane(p3);
        let p4 = to_plane(p4);
        let p5 = to_plane(p5);
        Some(graphene::Frustum::new(&p0, &p1, &p2, &p3, &p4, &p5))
    }
}

pub mod matrix {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("((dddd)(dddd)(dddd)(dddd))") })
    }
    pub fn to_variant(m: &graphene::Matrix) -> Variant {
        let [[ax, ay, az, aw], [bx, by, bz, bw], [cx, cy, cz, cw], [dx, dy, dz, dw]] = m.values();
        (
            (*ax as f64, *ay as f64, *az as f64, *aw as f64),
            (*bx as f64, *by as f64, *bz as f64, *bw as f64),
            (*cx as f64, *cy as f64, *cz as f64, *cw as f64),
            (*dx as f64, *dy as f64, *dz as f64, *dw as f64),
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Matrix> {
        let ((ax, ay, az, aw), (bx, by, bz, bw), (cx, cy, cz, cw), (dx, dy, dz, dw)) = variant
            .get::<(
                (f64, f64, f64, f64),
                (f64, f64, f64, f64),
                (f64, f64, f64, f64),
                (f64, f64, f64, f64),
            )>()?;
        Some(graphene::Matrix::from_float([
            ax as f32, ay as f32, az as f32, aw as f32, bx as f32, by as f32, bz as f32, bw as f32,
            cx as f32, cy as f32, cz as f32, cw as f32, dx as f32, dy as f32, dz as f32, dw as f32,
        ]))
    }
}

pub mod plane {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("((ddd)d)") })
    }
    pub fn to_variant(p: &graphene::Plane) -> Variant {
        let n = p.normal();
        (
            (n.x() as f64, n.y() as f64, n.z() as f64),
            p.constant() as f64,
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Plane> {
        let ((x, y, z), c) = variant.get::<((f64, f64, f64), f64)>()?;
        let n = graphene::Vec3::new(x as f32, y as f32, z as f32);
        Some(graphene::Plane::new(Some(&n), c as f32))
    }
}

pub mod point {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dd)") })
    }
    pub fn to_variant(p: &graphene::Point) -> Variant {
        (p.x() as f64, p.y() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Point> {
        let (x, y) = variant.get::<(f64, f64)>()?;
        Some(graphene::Point::new(x as f32, y as f32))
    }
}

pub mod point3d {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(ddd)") })
    }
    pub fn to_variant(p: &graphene::Point3D) -> Variant {
        (p.x() as f64, p.y() as f64, p.z() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Point3D> {
        let (x, y, z) = variant.get::<(f64, f64, f64)>()?;
        Some(graphene::Point3D::new(x as f32, y as f32, z as f32))
    }
}

pub mod quad {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("((dd)(dd)(dd)(dd))") })
    }
    pub fn to_variant(q: &graphene::Quad) -> Variant {
        let p = q.points();
        (
            (p[0].x() as f64, p[0].y() as f64),
            (p[1].x() as f64, p[1].y() as f64),
            (p[2].x() as f64, p[2].y() as f64),
            (p[3].x() as f64, p[3].y() as f64),
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Quad> {
        let ((x1, y1), (x2, y2), (x3, y3), (x4, y4)) =
            variant.get::<((f64, f64), (f64, f64), (f64, f64), (f64, f64))>()?;
        let p1 = graphene::Point::new(x1 as f32, y1 as f32);
        let p2 = graphene::Point::new(x2 as f32, y2 as f32);
        let p3 = graphene::Point::new(x3 as f32, y3 as f32);
        let p4 = graphene::Point::new(x4 as f32, y4 as f32);
        Some(graphene::Quad::new(&p1, &p2, &p3, &p4))
    }
}

pub mod quaternion {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddd)") })
    }
    pub fn to_variant(q: &graphene::Quaternion) -> Variant {
        (q.x() as f64, q.y() as f64, q.z() as f64, q.w() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Quaternion> {
        let (x, y, z, w) = variant.get::<(f64, f64, f64, f64)>()?;
        Some(graphene::Quaternion::new(
            x as f32, y as f32, z as f32, w as f32,
        ))
    }
}

pub mod ray {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("((ddd)(ddd))") })
    }
    pub fn to_variant(r: &graphene::Ray) -> Variant {
        let o = r.origin();
        let d = r.direction();
        (
            (o.x() as f64, o.y() as f64, o.z() as f64),
            (d.x() as f64, d.y() as f64, d.z() as f64),
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Ray> {
        let ((ox, oy, oz), (dx, dy, dz)) = variant.get::<((f64, f64, f64), (f64, f64, f64))>()?;
        let o = graphene::Point3D::new(ox as f32, oy as f32, oz as f32);
        let d = graphene::Vec3::new(dx as f32, dy as f32, dz as f32);
        Some(graphene::Ray::new(Some(&o), Some(&d)))
    }
}

pub mod rect {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddd)") })
    }
    pub fn to_variant(r: &graphene::Rect) -> Variant {
        (
            r.x() as f64,
            r.y() as f64,
            r.width() as f64,
            r.height() as f64,
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Rect> {
        let (x, y, w, h) = variant.get::<(f64, f64, f64, f64)>()?;
        Some(graphene::Rect::new(x as f32, y as f32, w as f32, h as f32))
    }
}

pub mod size {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dd)") })
    }
    pub fn to_variant(s: &graphene::Size) -> Variant {
        (s.width() as f64, s.height() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Size> {
        let (w, h) = variant.get::<(f64, f64)>()?;
        Some(graphene::Size::new(w as f32, h as f32))
    }
}

pub mod sphere {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("((ddd)d)") })
    }
    pub fn to_variant(s: &graphene::Sphere) -> Variant {
        let c = s.center();
        (
            (c.x() as f64, c.y() as f64, c.z() as f64),
            s.radius() as f64,
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Sphere> {
        let ((x, y, z), r) = variant.get::<((f64, f64, f64), f64)>()?;
        let n = graphene::Point3D::new(x as f32, y as f32, z as f32);
        Some(graphene::Sphere::new(Some(&n), r as f32))
    }
}

pub mod triangle {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("((ddd)(ddd)(ddd))") })
    }
    pub fn to_variant(t: &graphene::Triangle) -> Variant {
        let (a, b, c) = t.vertices();
        (
            (a.x() as f64, a.y() as f64, a.z() as f64),
            (b.x() as f64, b.y() as f64, b.z() as f64),
            (c.x() as f64, c.y() as f64, c.z() as f64),
        )
            .to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Triangle> {
        let ((ax, ay, az), (bx, by, bz), (cx, cy, cz)) =
            variant.get::<((f64, f64, f64), (f64, f64, f64), (f64, f64, f64))>()?;
        Some(graphene::Triangle::from_float(
            [ax as f32, ay as f32, az as f32],
            [bx as f32, by as f32, bz as f32],
            [cx as f32, cy as f32, cz as f32],
        ))
    }
}

pub mod vec2 {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dd)") })
    }
    pub fn to_variant(v: &graphene::Vec2) -> Variant {
        (v.x() as f64, v.y() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Vec2> {
        let (x, y) = variant.get::<(f64, f64)>()?;
        Some(graphene::Vec2::new(x as f32, y as f32))
    }
}

pub mod vec3 {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(ddd)") })
    }
    pub fn to_variant(v: &graphene::Vec3) -> Variant {
        (v.x() as f64, v.y() as f64, v.z() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Vec3> {
        let (x, y, z) = variant.get::<(f64, f64, f64)>()?;
        Some(graphene::Vec3::new(x as f32, y as f32, z as f32))
    }
}

pub mod vec4 {
    use super::*;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("(dddd)") })
    }
    pub fn to_variant(v: &graphene::Vec4) -> Variant {
        (v.x() as f64, v.y() as f64, v.z() as f64, v.w() as f64).to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<graphene::Vec4> {
        let (x, y, z, w) = variant.get::<(f64, f64, f64, f64)>()?;
        Some(graphene::Vec4::new(x as f32, y as f32, z as f32, w as f32))
    }
}
