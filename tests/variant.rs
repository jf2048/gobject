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
