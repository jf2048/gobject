
macro_rules! declare_optional {
    ($ty:ty) => {
        pub mod optional {
            pub fn serialize<S: serde::Serializer>(o: &Option<$ty>, s: S) -> Result<S::Ok, S::Error> {
                #[derive(serde::Serialize)]
                #[serde(transparent)]
                struct Writer<'w>(#[serde(with = "super")] &'w $ty);
                serde::Serialize::serialize(&o.as_ref().map(Writer), s)
            }
            pub fn deserialize<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<$ty>, D::Error> {
                #[derive(serde::Deserialize)]
                #[serde(transparent)]
                struct Reader(#[serde(with = "super")] $ty);
                <Option::<Reader> as serde::Deserialize>::deserialize(d).map(|o| o.map(|o| o.0))
            }
        }
    };
}

pub mod glib;

#[cfg(feature = "use_cairo")]
pub mod cairo;
#[cfg(feature = "use_gdk4")]
pub mod gdk4;
#[cfg(feature = "use_gio")]
pub mod gio;
#[cfg(feature = "use_graphene")]
pub mod graphene;
#[cfg(feature = "use_gtk4")]
pub mod gtk4;
