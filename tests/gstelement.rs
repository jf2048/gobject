#![cfg(feature = "use_gst")]

use glib::StaticType;

#[gobject::gst_element(
    class(final),
    factory_name = "testelement",
    rank = "Primary",
    long_name = "TheTestElement",
    classification = "Test/Filter",
    description = "Just a test",
    author = "Thibault Saunier <tsaunier@igalia.com>",
    pad_templates(
        src(presence="Always"),
        // `__` is transformed to `_%` as "%" is not a valid character
        sink__u(direction="Sink", presence="Sometimes", caps="video/x-raw"),
    ),
    debug_category_colors(gst::DebugColorFlags::FG_BLUE),
)]
mod imp {
    use once_cell::sync::Lazy;
    use std::str::FromStr;
    use std::sync::Mutex;

    struct TestElement {
        #[property(get, set)]
        url: Mutex<String>,

        #[property(get, set, blurb = "The names")]
        names: Mutex<gst::Array>,

        #[property(get, set, blurb = "The framerate")]
        framerate: Mutex<gst::Fraction>,
    }

    impl Default for TestElement {
        fn default() -> Self {
            let values: Vec<String> = Default::default();
            Self {
                url: Default::default(),
                names: Mutex::new(gst::Array::new(&values)),
                framerate: Mutex::new(gst::Fraction::new(30, 1)),
            }
        }
    }

    impl TestElement {
        fn constructed(&self, obj: &super::TestElement) {
            gst::info!(CAT, obj: obj, "Test element is constructed")
        }
    }
}

#[cfg(feature = "use_gst")]
#[test]
fn element() {
    use gst::prelude::*;
    use std::cmp;

    gst::init().unwrap();
    register(None).unwrap();
    let element = gst::ElementFactory::make("testelement", None)
        .expect("testelement should have been registered");

    let template = element.pad_template("src").unwrap();
    assert_eq!(template.direction(), gst::PadDirection::Src);
    assert_eq!(template.caps().clone(), gst::Caps::new_any());

    let template = element.pad_template("sink_%u").unwrap();
    assert_eq!(template.direction(), gst::PadDirection::Sink);
    assert_eq!(
        template.caps().clone(),
        gst::Caps::new_simple("video/x-raw", &[])
    );

    let pspec = element.find_property("names").unwrap();
    assert_eq!(pspec.blurb().unwrap(), "The names");

    let names = element.property::<gst::Array>("names");
    assert_eq!(names.len(), 0,);

    element.set_property_from_str("names", "<first, second>");
    let names = element.property::<gst::Array>("names");
    let v = gst::Array::from_values(["first".into(), "second".into()]);
    assert!(
        names.to_value().compare(&v.to_value()) == Some(cmp::Ordering::Equal),
        "{names:?} != {v:?}"
    );

    let pspec = element.find_property("framerate").unwrap();
    assert_eq!(pspec.blurb().unwrap(), "The framerate");

    let v = gst::Fraction::new(30, 1);
    element.set_property("framerate", &v);
    let framerate = element.property::<gst::Fraction>("framerate");
    assert!(
        framerate.to_value().compare(&v.to_value()) == Some(cmp::Ordering::Equal),
        "{framerate:?} != {v:?}"
    );
}
