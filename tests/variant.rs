#![cfg(feature = "variant")]

use glib::Cast;
use glib::StaticVariantType;
use glib::ToVariant;

#[gobject::class(abstract)]
mod obj_abstract {
    #[derive(Default)]
    #[properties]
    #[variant(from, to, child_types(super::ObjFinal))]
    pub struct ObjAbstract {
        #[property(get, set, abstract)]
        my_prop: std::marker::PhantomData<u64>,
    }
}

#[gobject::class(final, extends(ObjAbstract))]
mod obj_final {
    #[derive(Default)]
    #[variant(from, to)]
    pub struct ObjFinal {
        #[property(get, set, override_class = "super::ObjAbstract")]
        my_prop: std::cell::Cell<u64>,
        #[property(get, set)]
        str_prop: std::cell::RefCell<String>,
    }
    impl super::ObjAbstractImpl for ObjFinal {}
}

#[test]
fn final_variant() {
    assert_eq!("((t)s)", ObjFinal::static_variant_type().as_str());
    assert_eq!("(sv)", ObjAbstract::static_variant_type().as_str());

    let obj = glib::Object::new::<ObjFinal>(&[]).unwrap();
    obj.set_my_prop(64);
    obj.set_str_prop("hello".into());
    let result = obj.to_variant();
    assert_eq!(result.to_string(), "((uint64 64,), 'hello')");
    let other: ObjFinal = result.get().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");

    let result = obj.upcast_ref::<ObjAbstract>().to_variant();
    assert_eq!(
        result.to_string(),
        "('ObjFinal', <((uint64 64,), 'hello')>)"
    );
    let other: ObjAbstract = result.get().unwrap();
    let other = other.downcast::<ObjFinal>().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");
}

#[gobject::class(abstract)]
mod obj_abstract_dict {
    #[derive(Default)]
    #[properties]
    #[variant(from, to, dict, child_types(super::ObjFinalDict))]
    pub struct ObjAbstractDict {
        #[property(get, set, abstract)]
        my_prop: std::marker::PhantomData<u64>,
    }
}

#[gobject::class(final, extends(ObjAbstractDict))]
mod obj_final_dict {
    #[derive(Default)]
    #[variant(from, to, dict)]
    pub struct ObjFinalDict {
        #[property(get, set, override_class = "super::ObjAbstractDict")]
        my_prop: std::cell::Cell<u64>,
        #[property(get, set)]
        str_prop: std::cell::RefCell<String>,
    }
    impl super::ObjAbstractDictImpl for ObjFinalDict {}
}

#[test]
fn final_variant_dict() {
    assert_eq!("a{sv}", ObjFinalDict::static_variant_type().as_str());
    assert_eq!("(sv)", ObjAbstractDict::static_variant_type().as_str());

    let obj = glib::Object::new::<ObjFinalDict>(&[]).unwrap();
    obj.set_my_prop(64);
    obj.set_str_prop("hello".into());
    let result = obj.to_variant();
    assert_eq!(
        result.to_string(),
        "{'parent': <{'my-prop': <uint64 64>}>, 'str-prop': <'hello'>}",
    );
    let other: ObjFinalDict = result.get().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");

    let result = obj.upcast_ref::<ObjAbstractDict>().to_variant();
    assert_eq!(
        result.to_string(),
        "('ObjFinalDict', <{'parent': <{'my-prop': <uint64 64>}>, 'str-prop': <'hello'>}>)"
    );
    let other: ObjAbstractDict = result.get().unwrap();
    let other = other.downcast::<ObjFinalDict>().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");
}

#[gobject::class]
mod obj_derivable {
    #[derive(Default)]
    #[properties]
    #[variant(from, to, child_types(super::ObjFinal2))]
    pub struct ObjDerivable {
        #[property(get, set)]
        my_prop: std::cell::Cell<u64>,
    }
}

#[gobject::class(final, extends(ObjDerivable))]
mod obj_final2 {
    #[derive(Default)]
    #[variant(from, to)]
    pub struct ObjFinal2 {
        #[property(get, set)]
        str_prop: std::cell::RefCell<String>,
    }
    impl super::ObjDerivableImpl for ObjFinal2 {}
}

#[test]
fn derivable_variant() {
    assert_eq!("(sv)", ObjDerivable::static_variant_type().as_str());
    assert_eq!("((t)s)", ObjFinal2::static_variant_type().as_str());

    let obj = glib::Object::new::<ObjDerivable>(&[]).unwrap();
    obj.set_my_prop(123);
    let result = obj.to_variant();
    assert_eq!(result.to_string(), "('ObjDerivable', <(uint64 123,)>)");
    let other: ObjDerivable = result.get().unwrap();
    assert_eq!(other.my_prop(), 123);

    let obj = glib::Object::new::<ObjFinal2>(&[]).unwrap();
    obj.set_my_prop(64);
    obj.set_str_prop("hello".into());
    let result = obj.to_variant();
    assert_eq!(result.to_string(), "((uint64 64,), 'hello')");
    let other: ObjFinal2 = result.get().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");

    let result = obj.upcast_ref::<ObjDerivable>().to_variant();
    assert_eq!(
        result.to_string(),
        "('ObjFinal2', <((uint64 64,), 'hello')>)"
    );
    let other: ObjDerivable = result.get().unwrap();
    let other = other.downcast::<ObjFinal2>().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");
}

#[gobject::class]
mod obj_derivable_dict {
    #[derive(Default)]
    #[properties]
    #[variant(from, to, dict, child_types(super::ObjFinal2Dict))]
    pub struct ObjDerivableDict {
        #[property(get, set)]
        my_prop: std::cell::Cell<u64>,
    }
}

#[gobject::class(final, extends(ObjDerivableDict))]
mod obj_final2_dict {
    #[derive(Default)]
    #[variant(from, to, dict)]
    pub struct ObjFinal2Dict {
        #[property(get, set)]
        str_prop: std::cell::RefCell<String>,
    }
    impl super::ObjDerivableDictImpl for ObjFinal2Dict {}
}

#[test]
fn derivable_variant_dict() {
    assert_eq!("(sv)", ObjDerivableDict::static_variant_type().as_str());
    assert_eq!("a{sv}", ObjFinal2Dict::static_variant_type().as_str());

    let obj = glib::Object::new::<ObjDerivableDict>(&[]).unwrap();
    obj.set_my_prop(123);
    let result = obj.to_variant();
    assert_eq!(
        result.to_string(),
        "('ObjDerivableDict', <{'my-prop': <uint64 123>}>)"
    );
    let other: ObjDerivableDict = result.get().unwrap();
    assert_eq!(other.my_prop(), 123);

    let obj = glib::Object::new::<ObjFinal2Dict>(&[]).unwrap();
    obj.set_my_prop(64);
    obj.set_str_prop("hello".into());
    let result = obj.to_variant();
    assert_eq!(
        result.to_string(),
        "{'parent': <{'my-prop': <uint64 64>}>, 'str-prop': <'hello'>}"
    );
    let other: ObjFinal2Dict = result.get().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");

    let result = obj.upcast_ref::<ObjDerivableDict>().to_variant();
    assert_eq!(
        result.to_string(),
        "('ObjFinal2Dict', <{'parent': <{'my-prop': <uint64 64>}>, 'str-prop': <'hello'>}>)"
    );
    let other: ObjDerivableDict = result.get().unwrap();
    let other = other.downcast::<ObjFinal2Dict>().unwrap();
    assert_eq!(other.my_prop(), 64);
    assert_eq!(other.str_prop(), "hello");
}

#[gobject::class(final)]
mod conversions {
    #[derive(Default)]
    #[variant(from, to)]
    pub struct Conversions {
        #[property(get, set, boxed)]
        #[variant(with = "gobject::variant::glib::date_time::optional")]
        datetime: std::cell::RefCell<Option<glib::DateTime>>,
    }
}

#[test]
fn convert_paths() {
    let obj = glib::Object::new::<Conversions>(&[]).unwrap();
    obj.set_datetime(Some(
        glib::DateTime::new(&glib::TimeZone::utc(), 1980, 1, 1, 0, 0, 0.).unwrap(),
    ));
    let result = obj.to_variant();
    assert_eq!(result.to_string(), "(@ms '1980-01-01T00:00:00Z',)");
    let other: Conversions = result.get().unwrap();
    assert_eq!(other.datetime().unwrap().year(), 1980);
}
