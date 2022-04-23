#![cfg(feature = "use_serde")]

use glib::Cast;

#[gobject::class(abstract)]
mod obj_abstract {
    #[derive(Default)]
    #[properties]
    #[gobject_serde(serialize, deserialize, child_types(super::ObjFinal, super::ObjFinal2))]
    pub struct ObjAbstract {
        #[property(get, set, abstract)]
        my_prop: std::marker::PhantomData<u64>,
    }
}

#[gobject::class(final, extends(ObjAbstract))]
mod obj_final {
    #[derive(Default)]
    #[gobject_serde(serialize, deserialize)]
    pub struct ObjFinal {
        #[property(get, set, override_class = "super::ObjAbstract")]
        my_prop: std::cell::Cell<u64>,
        #[property(get, set)]
        str_prop: std::cell::RefCell<String>,
    }
    impl super::ObjAbstractImpl for ObjFinal {}
}

#[gobject::class(final, extends(ObjAbstract))]
mod obj_final2 {
    #[derive(Default)]
    #[gobject_serde(serialize, deserialize)]
    pub struct ObjFinal2 {
        #[property(get, set, override_class = "super::ObjAbstract")]
        my_prop: std::cell::Cell<u64>,
        #[property(get, set, boxed)]
        #[serde(with = "gobject_serde::glib::date::optional")]
        date: std::cell::RefCell<Option<glib::Date>>,
    }
    impl super::ObjAbstractImpl for ObjFinal2 {}
}

#[test]
fn final_json() {
    let obj = glib::Object::new::<ObjFinal>(&[]).unwrap();
    obj.set_my_prop(64);
    obj.set_str_prop("hello".into());
    let result = serde_json::to_string(&obj).unwrap();
    assert_eq!(result, r#"{"parent":{"my_prop":64},"str_prop":"hello"}"#);
    let other: ObjFinal = serde_json::from_str(&result).unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");

    let result = serde_json::to_string(obj.upcast_ref::<ObjAbstract>()).unwrap();
    assert_eq!(
        result,
        r#"{"ObjFinal":{"parent":{"my_prop":64},"str_prop":"hello"}}"#
    );
    let other: ObjAbstract = serde_json::from_str(&result).unwrap();
    let other = other.downcast::<ObjFinal>().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");
}

#[test]
fn final2_json() {
    let obj2 = glib::Object::new::<ObjFinal2>(&[]).unwrap();
    obj2.set_my_prop(128);
    let result = serde_json::to_string(&obj2).unwrap();
    assert_eq!(result, r#"{"parent":{"my_prop":128},"date":null}"#);
    obj2.set_date(Some(
        glib::Date::from_dmy(1, glib::DateMonth::January, 1980).unwrap(),
    ));
    let result = serde_json::to_string(&obj2).unwrap();
    assert_eq!(result, r#"{"parent":{"my_prop":128},"date":722815}"#);
    let other2: ObjFinal2 = serde_json::from_str(&result).unwrap();
    assert_eq!(other2.my_prop(), 128);
    assert_eq!(other2.date().map(|d| d.year()), Some(1980));

    let result = serde_json::to_string(obj2.upcast_ref::<ObjAbstract>()).unwrap();
    assert_eq!(
        result,
        r#"{"ObjFinal2":{"parent":{"my_prop":128},"date":722815}}"#
    );
    let other2: ObjAbstract = serde_json::from_str(&result).unwrap();
    let other2 = other2.downcast::<ObjFinal2>().unwrap();
    assert_eq!(other2.my_prop(), 128);
    assert_eq!(other2.date().map(|d| d.year()), Some(1980));
}

#[gobject::class]
mod obj_derivable {
    #[derive(Default)]
    #[properties]
    #[gobject_serde(serialize, deserialize, child_types(super::ObjFinal3))]
    pub struct ObjDerivable {
        #[property(get, set)]
        my_prop3: std::cell::Cell<u64>,
    }
}

#[gobject::class(final, extends(ObjDerivable))]
mod obj_final3 {
    #[derive(Default)]
    #[gobject_serde(serialize, deserialize)]
    pub struct ObjFinal3 {
        #[property(get, set)]
        str_prop3: std::cell::RefCell<String>,
    }
    impl super::ObjDerivableImpl for ObjFinal3 {}
}
