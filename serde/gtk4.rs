use gtk4::prelude::*;
use serde::ser::SerializeSeq;
use serde::{de, ser, Deserialize, Deserializer, Serialize, Serializer};

pub mod adjustment {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gtk4::Adjustment")]
    struct Adjustment(f64, f64, f64, f64, f64, f64);
    pub fn serialize<S: Serializer>(a: &gtk4::Adjustment, s: S) -> Result<S::Ok, S::Error> {
        Adjustment(
            a.value(),
            a.lower(),
            a.upper(),
            a.step_increment(),
            a.page_increment(),
            a.page_size(),
        )
        .serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gtk4::Adjustment, D::Error> {
        let Adjustment(v, l, u, si, pi, ps) = Adjustment::deserialize(d)?;
        Ok(gtk4::Adjustment::new(v, l, u, si, pi, ps))
    }
    declare_optional!(gtk4::Adjustment);
}

pub mod border {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gtk4::Border")]
    struct Border(i16, i16, i16, i16);
    pub fn serialize<S: Serializer>(b: &gtk4::Border, s: S) -> Result<S::Ok, S::Error> {
        Border(b.left(), b.right(), b.top(), b.bottom()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gtk4::Border, D::Error> {
        let Border(l, r, t, b) = Border::deserialize(d)?;
        Ok(gtk4::Border::builder()
            .left(l)
            .right(r)
            .top(t)
            .bottom(b)
            .build())
    }
    declare_optional!(gtk4::Border);
}

pub mod paper_size {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gtk4::PaperSize")]
    struct PaperSize<'k>(&'k str);
    pub fn serialize<S: Serializer>(ps: &gtk4::PaperSize, s: S) -> Result<S::Ok, S::Error> {
        let kf = glib::KeyFile::new();
        ps.clone().to_key_file(&kf, "paper_size");
        PaperSize(kf.to_data().as_str()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gtk4::PaperSize, D::Error> {
        let d = PaperSize::deserialize(d)?.0;
        let kf = glib::KeyFile::new();
        glib::KeyFile::load_from_data(&kf, d, glib::KeyFileFlags::NONE)
            .map_err(de::Error::custom)?;
        gtk4::PaperSize::from_key_file(&kf, Some("paper_size")).map_err(de::Error::custom)
    }
    declare_optional!(gtk4::PaperSize);
}

pub mod string_object {
    use super::*;
    pub fn serialize<S: Serializer>(so: &gtk4::StringObject, s: S) -> Result<S::Ok, S::Error> {
        so.string().serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gtk4::StringObject, D::Error> {
        Ok(gtk4::StringObject::new(Deserialize::deserialize(d)?))
    }
    declare_optional!(gtk4::StringObject);
}

pub mod string_list {
    use super::*;
    pub fn serialize<S: Serializer>(sl: &gtk4::StringList, s: S) -> Result<S::Ok, S::Error> {
        let count = sl.n_items();
        let mut seq = s.serialize_seq(Some(count as usize))?;
        for i in 0..count {
            let st = sl.string(i).ok_or_else(|| {
                ser::Error::custom(format!("Unexpected end of StringList at index {}", i))
            })?;
            seq.serialize_element(st.as_str())?;
        }
        seq.end()
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gtk4::StringList, D::Error> {
        struct Visitor;

        impl<'de> de::Visitor<'de> for Visitor {
            type Value = gtk4::StringList;
            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("a sequence")
            }
            fn visit_seq<A>(self, mut seq: A) -> Result<Self::Value, A::Error>
            where
                A: de::SeqAccess<'de>,
            {
                let sl = gtk4::StringList::new(&[]);

                while let Some(value) = seq.next_element()? {
                    sl.append(value);
                }

                Ok(sl)
            }
        }

        d.deserialize_seq(Visitor)
    }
    declare_optional!(gtk4::StringList);
}
