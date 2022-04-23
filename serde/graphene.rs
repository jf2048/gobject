use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod box_ {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Box")]
    struct Box((f32, f32, f32), (f32, f32, f32));
    pub fn serialize<S: Serializer>(b: &graphene::Box, s: S) -> Result<S::Ok, S::Error> {
        let a = b.min();
        let b = b.max();
        Box((a.x(), a.y(), a.z()), (b.x(), b.y(), b.z())).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Box, D::Error> {
        let Box((ax, ay, az), (bx, by, bz)) = Box::deserialize(d)?;
        let a = graphene::Point3D::new(ax, ay, az);
        let b = graphene::Point3D::new(bx, by, bz);
        Ok(graphene::Box::new(Some(&a), Some(&b)))
    }
    declare_optional!(graphene::Box);
}

pub mod euler {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Euler")]
    struct Euler(f32, f32, f32);
    pub fn serialize<S: Serializer>(e: &graphene::Euler, s: S) -> Result<S::Ok, S::Error> {
        Euler(e.x(), e.y(), e.z()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Euler, D::Error> {
        let Euler(x, y, z) = Euler::deserialize(d)?;
        Ok(graphene::Euler::new(x, y, z))
    }
    declare_optional!(graphene::Euler);
}

pub mod frustum {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Frustum")]
    struct Frustum(
        ((f32, f32, f32), f32),
        ((f32, f32, f32), f32),
        ((f32, f32, f32), f32),
        ((f32, f32, f32), f32),
        ((f32, f32, f32), f32),
        ((f32, f32, f32), f32),
    );
    pub fn serialize<S: Serializer>(f: &graphene::Frustum, s: S) -> Result<S::Ok, S::Error> {
        let p = f.planes();
        #[inline]
        fn to_tuple(p: &graphene::Plane) -> ((f32, f32, f32), f32) {
            let n = p.normal();
            ((n.x(), n.y(), n.z()), p.constant())
        }
        Frustum(
            to_tuple(&p[0]),
            to_tuple(&p[1]),
            to_tuple(&p[2]),
            to_tuple(&p[3]),
            to_tuple(&p[4]),
            to_tuple(&p[5]),
        )
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Frustum, D::Error> {
        let Frustum(p0, p1, p2, p3, p4, p5) = Frustum::deserialize(d)?;
        #[inline]
        fn to_plane(((x, y, z), c): ((f32, f32, f32), f32)) -> graphene::Plane {
            graphene::Plane::new(Some(&graphene::Vec3::new(x, y, z)), c)
        }
        let p0 = to_plane(p0);
        let p1 = to_plane(p1);
        let p2 = to_plane(p2);
        let p3 = to_plane(p3);
        let p4 = to_plane(p4);
        let p5 = to_plane(p5);
        Ok(graphene::Frustum::new(&p0, &p1, &p2, &p3, &p4, &p5))
    }
    declare_optional!(graphene::Frustum);
}

pub mod matrix {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Matrix")]
    struct Matrix(
        (f32, f32, f32, f32),
        (f32, f32, f32, f32),
        (f32, f32, f32, f32),
        (f32, f32, f32, f32),
    );
    pub fn serialize<S: Serializer>(m: &graphene::Matrix, s: S) -> Result<S::Ok, S::Error> {
        let [[ax, ay, az, aw], [bx, by, bz, bw], [cx, cy, cz, cw], [dx, dy, dz, dw]] = m.values();
        Matrix(
            (*ax, *ay, *az, *aw),
            (*bx, *by, *bz, *bw),
            (*cx, *cy, *cz, *cw),
            (*dx, *dy, *dz, *dw),
        )
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Matrix, D::Error> {
        let Matrix((ax, ay, az, aw), (bx, by, bz, bw), (cx, cy, cz, cw), (dx, dy, dz, dw)) =
            Matrix::deserialize(d)?;
        Ok(graphene::Matrix::from_float([
            ax, ay, az, aw, bx, by, bz, bw, cx, cy, cz, cw, dx, dy, dz, dw,
        ]))
    }
    declare_optional!(graphene::Matrix);
}

pub mod plane {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Plane")]
    struct Plane((f32, f32, f32), f32);
    pub fn serialize<S: Serializer>(p: &graphene::Plane, s: S) -> Result<S::Ok, S::Error> {
        let n = p.normal();
        Plane((n.x(), n.y(), n.z()), p.constant()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Plane, D::Error> {
        let Plane((x, y, z), c) = Plane::deserialize(d)?;
        let n = graphene::Vec3::new(x, y, z);
        Ok(graphene::Plane::new(Some(&n), c))
    }
    declare_optional!(graphene::Plane);
}

pub mod point {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Point")]
    struct Point(f32, f32);
    pub fn serialize<S: Serializer>(p: &graphene::Point, s: S) -> Result<S::Ok, S::Error> {
        Point(p.x(), p.y()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Point, D::Error> {
        let Point(x, y) = Point::deserialize(d)?;
        Ok(graphene::Point::new(x, y))
    }
    declare_optional!(graphene::Point);
}

pub mod point3d {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Point3D")]
    struct Point3D(f32, f32, f32);
    pub fn serialize<S: Serializer>(p: &graphene::Point3D, s: S) -> Result<S::Ok, S::Error> {
        Point3D(p.x(), p.y(), p.z()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Point3D, D::Error> {
        let Point3D(x, y, z) = Point3D::deserialize(d)?;
        Ok(graphene::Point3D::new(x, y, z))
    }
    declare_optional!(graphene::Point3D);
}

pub mod quad {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Quad")]
    struct Quad((f32, f32), (f32, f32), (f32, f32), (f32, f32));
    pub fn serialize<S: Serializer>(q: &graphene::Quad, s: S) -> Result<S::Ok, S::Error> {
        let p = q.points();
        Quad(
            (p[0].x(), p[0].y()),
            (p[1].x(), p[1].y()),
            (p[2].x(), p[2].y()),
            (p[3].x(), p[3].y()),
        )
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Quad, D::Error> {
        let Quad((x1, y1), (x2, y2), (x3, y3), (x4, y4)) = Quad::deserialize(d)?;
        let p1 = graphene::Point::new(x1, y1);
        let p2 = graphene::Point::new(x2, y2);
        let p3 = graphene::Point::new(x3, y3);
        let p4 = graphene::Point::new(x4, y4);
        Ok(graphene::Quad::new(&p1, &p2, &p3, &p4))
    }
    declare_optional!(graphene::Quad);
}

pub mod quaternion {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Quaternion")]
    struct Quaternion(f32, f32, f32, f32);
    pub fn serialize<S: Serializer>(q: &graphene::Quaternion, s: S) -> Result<S::Ok, S::Error> {
        Quaternion(q.x(), q.y(), q.z(), q.w()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Quaternion, D::Error> {
        let Quaternion(x, y, z, w) = Quaternion::deserialize(d)?;
        Ok(graphene::Quaternion::new(x, y, z, w))
    }
    declare_optional!(graphene::Quaternion);
}

pub mod ray {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Ray")]
    struct Ray((f32, f32, f32), (f32, f32, f32));
    pub fn serialize<S: Serializer>(r: &graphene::Ray, s: S) -> Result<S::Ok, S::Error> {
        let o = r.origin();
        let d = r.direction();
        Ray((o.x(), o.y(), o.z()), (d.x(), d.y(), d.z())).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Ray, D::Error> {
        let Ray((ox, oy, oz), (dx, dy, dz)) = Ray::deserialize(d)?;
        let o = graphene::Point3D::new(ox, oy, oz);
        let d = graphene::Vec3::new(dx, dy, dz);
        Ok(graphene::Ray::new(Some(&o), Some(&d)))
    }
    declare_optional!(graphene::Ray);
}

pub mod rect {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Rect")]
    struct Rect(f32, f32, f32, f32);
    pub fn serialize<S: Serializer>(r: &graphene::Rect, s: S) -> Result<S::Ok, S::Error> {
        Rect(r.x(), r.y(), r.width(), r.height()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Rect, D::Error> {
        let Rect(x, y, w, h) = Rect::deserialize(d)?;
        Ok(graphene::Rect::new(x, y, w, h))
    }
    declare_optional!(graphene::Rect);
}

pub mod size {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Size")]
    struct Size(f32, f32);
    pub fn serialize<S: Serializer>(sz: &graphene::Size, s: S) -> Result<S::Ok, S::Error> {
        Size(sz.width(), sz.height()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Size, D::Error> {
        let Size(w, h) = Size::deserialize(d)?;
        Ok(graphene::Size::new(w, h))
    }
    declare_optional!(graphene::Size);
}

pub mod sphere {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Sphere")]
    struct Sphere((f32, f32, f32), f32);
    pub fn serialize<S: Serializer>(sp: &graphene::Sphere, s: S) -> Result<S::Ok, S::Error> {
        let c = sp.center();
        Sphere((c.x(), c.y(), c.z()), sp.radius()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Sphere, D::Error> {
        let Sphere((x, y, z), r) = Sphere::deserialize(d)?;
        let c = graphene::Point3D::new(x, y, z);
        Ok(graphene::Sphere::new(Some(&c), r))
    }
    declare_optional!(graphene::Sphere);
}

pub mod triangle {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Triangle")]
    struct Triangle((f32, f32, f32), (f32, f32, f32), (f32, f32, f32));
    pub fn serialize<S: Serializer>(t: &graphene::Triangle, s: S) -> Result<S::Ok, S::Error> {
        let (a, b, c) = t.vertices();
        Triangle(
            (a.x(), a.y(), a.z()),
            (b.x(), b.y(), b.z()),
            (c.x(), c.y(), c.z()),
        )
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Triangle, D::Error> {
        let Triangle((ax, ay, az), (bx, by, bz), (cx, cy, cz)) = Triangle::deserialize(d)?;
        Ok(graphene::Triangle::from_float(
            [ax, ay, az],
            [bx, by, bz],
            [cx, cy, cz],
        ))
    }
    declare_optional!(graphene::Triangle);
}

pub mod vec2 {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Vec2")]
    struct Vec2(f32, f32);
    pub fn serialize<S: Serializer>(v: &graphene::Vec2, s: S) -> Result<S::Ok, S::Error> {
        Vec2(v.x(), v.y()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Vec2, D::Error> {
        let Vec2(x, y) = Vec2::deserialize(d)?;
        Ok(graphene::Vec2::new(x, y))
    }
    declare_optional!(graphene::Vec2);
}

pub mod vec3 {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Vec3")]
    struct Vec3(f32, f32, f32);
    pub fn serialize<S: Serializer>(v: &graphene::Vec3, s: S) -> Result<S::Ok, S::Error> {
        Vec3(v.x(), v.y(), v.z()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Vec3, D::Error> {
        let Vec3(x, y, z) = Vec3::deserialize(d)?;
        Ok(graphene::Vec3::new(x, y, z))
    }
    declare_optional!(graphene::Vec3);
}

pub mod vec4 {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "graphene::Vec4")]
    struct Vec4(f32, f32, f32, f32);
    pub fn serialize<S: Serializer>(v: &graphene::Vec4, s: S) -> Result<S::Ok, S::Error> {
        Vec4(v.x(), v.y(), v.z(), v.w()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<graphene::Vec4, D::Error> {
        let Vec4(x, y, z, w) = Vec4::deserialize(d)?;
        Ok(graphene::Vec4::new(x, y, z, w))
    }
    declare_optional!(graphene::Vec4);
}
