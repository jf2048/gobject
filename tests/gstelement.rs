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
    use std::sync::Mutex;

    use once_cell::sync::Lazy;
    use std::str::FromStr;

    #[derive(Default)]
    struct TestElement {
        #[property(get, set)]
        url: Mutex<String>,
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
}
