use serde::{Deserialize, Deserializer, Serialize, Serializer};

pub mod rectangle {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gdk4::Rectangle")]
    struct Rectangle(i32, i32, i32, i32);
    pub fn serialize<S: Serializer>(r: &gdk4::Rectangle, s: S) -> Result<S::Ok, S::Error> {
        Rectangle(r.x(), r.y(), r.width(), r.height()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gdk4::Rectangle, D::Error> {
        let Rectangle(x, y, w, h) = Rectangle::deserialize(d)?;
        Ok(gdk4::Rectangle::new(x, y, w, h))
    }
}

pub mod rgba {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gdk4::RGBA")]
    struct RGBA(f32, f32, f32, f32);
    pub fn serialize<S: Serializer>(c: &gdk4::RGBA, s: S) -> Result<S::Ok, S::Error> {
        RGBA(c.red(), c.green(), c.blue(), c.alpha()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gdk4::RGBA, D::Error> {
        let RGBA(r, g, b, a) = RGBA::deserialize(d)?;
        Ok(gdk4::RGBA::new(r, g, b, a))
    }
}
