use glib::prelude::*;

#[gobject::class(final)]
mod basic {
    use glib::once_cell::unsync::OnceCell;
    use glib::subclass::prelude::ObjectImplExt;
    use glib::subclass::prelude::ObjectSubclassIsExt;
    use std::cell::{Cell, RefCell};
    use std::marker::PhantomData;
    use std::sync::{Mutex, RwLock};

    #[properties]
    #[derive(Default)]
    pub struct BasicProps {
        #[property(get)]
        readable_i32: Cell<i32>,
        #[property(set)]
        writable_i32: Cell<i32>,
        #[property(get, set)]
        my_i32: Cell<i32>,
        #[property(get, set, borrow)]
        my_str: RefCell<String>,
        #[property(get, set)]
        my_mutex: Mutex<i32>,
        #[property(get, set)]
        my_rw_lock: RwLock<String>,
        #[property(
            get,
            set,
            construct,
            name = "my-u8",
            nick = "My U8",
            blurb = "A uint8",
            builder(minimum = 5, maximum = 20, default_value = 19)
        )]
        my_attributed: Cell<u8>,
        #[property(get, set, construct_only, builder(default_value = 100.0))]
        my_construct_only: Cell<f64>,
        #[property(get, set, lax_validation)]
        my_lax: Cell<u32>,
        #[property(get, set, explicit_notify, lax_validation)]
        my_explicit: Cell<u64>,
        #[property(get, set, explicit_notify, lax_validation)]
        my_auto_set: OnceCell<f32>,
        #[property(get, set, explicit_notify, lax_validation, construct_only)]
        my_auto_set_co: OnceCell<f32>,
        #[property(get = "_", set = "_", explicit_notify, lax_validation)]
        my_custom_accessors: RefCell<String>,
        #[property(computed, get, set, explicit_notify)]
        my_computed_prop: PhantomData<i32>,
        #[property(get, set, storage = "inner.my_bool")]
        my_delegate: Cell<bool>,
        #[property(get, set, notify = false, connect_notify = false)]
        my_no_defaults: Cell<u64>,

        inner: BasicPropsInner,
    }

    #[derive(Default)]
    struct BasicPropsInner {
        my_bool: Cell<bool>,
    }

    impl BasicProps {
        fn constructed(&self, obj: &super::BasicProps) {
            self.parent_constructed(obj);
            obj.connect_my_i32_notify(|obj| obj.notify_my_computed_prop());
        }
        #[public]
        fn my_custom_accessors(&self) -> String {
            self.my_custom_accessors.borrow().clone()
        }
        #[public]
        fn set_my_custom_accessors(obj: &super::BasicProps, value: String) {
            let imp = obj.imp();
            let old = imp.my_custom_accessors.replace(value);
            if old != *imp.my_custom_accessors.borrow() {
                obj.notify_my_custom_accessors();
            }
        }
        #[public]
        fn my_computed_prop(&self) -> i32 {
            self.my_i32.get() + 7
        }
        fn set_my_computed_prop(obj: &super::BasicProps, value: i32) {
            obj.set_my_i32(value - 7);
        }
    }
}

#[test]
fn basic_properties() {
    use glib::subclass::prelude::ObjectImpl;

    let props = glib::Object::new::<BasicProps>(&[]).unwrap();
    assert_eq!(<basic::BasicProps as ObjectImpl>::properties().len(), 16);
    assert_eq!(props.list_properties().len(), 16);
    props.connect_my_i32_notify(|props| props.set_my_str("Updated".into()));
    assert_eq!(props.my_str(), "");
    props.set_my_i32(5);
    assert_eq!(props.my_i32(), 5);
    assert_eq!(props.property::<i32>("my-i32"), 5);
    assert_eq!(props.my_computed_prop(), 12);
    props.set_my_computed_prop(400);
    assert_eq!(props.property::<i32>("my-i32"), 393);
    assert_eq!(props.property::<i32>("my-computed-prop"), 400);
    assert_eq!(props.my_str(), "Updated");
    assert_eq!(*props.borrow_my_str(), "Updated");
    assert_eq!(props.property::<String>("my-str"), "Updated");
    assert_eq!(props.my_u8(), 19);
    assert_eq!(props.my_construct_only(), 100.0);
}

#[gobject::class(abstract)]
mod base {
    use std::marker::PhantomData;

    #[derive(Default)]
    pub struct BaseObject {
        #[property(
            name = "renamed-string",
            abstract,
            get,
            set,
            construct,
            builder(default_value("Some(\"foobar\")"))
        )]
        a_string: PhantomData<String>,
    }
}

#[derive(Debug, Eq, PartialEq, Clone, Copy, glib::Enum)]
#[repr(u32)]
#[enum_type(name = "TestAnimalType")]
pub enum Animal {
    Goat,
    Dog,
    Cat,
    Badger,
}

#[gobject::class(final, extends(BaseObject))]
mod small {
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct SmallObject {
        #[property(get, set, override_class = "super::BaseObject")]
        renamed_string: RefCell<String>,
    }
    impl super::BaseObjectImpl for SmallObject {}
}

#[gobject::class(final, extends(BaseObject))]
mod complex {
    use glib::once_cell::unsync::OnceCell;
    use glib::subclass::prelude::ObjectImpl;
    use glib::{StaticType, ToVariant};
    use std::cell::{Cell, RefCell};

    pub struct ComplexProps {
        #[property(get, set, builder(is_a_type("glib::Type::OBJECT")))]
        object_type: Cell<glib::Type>,
        #[property(get, set, boxed)]
        time: RefCell<glib::DateTime>,
        #[property(get, set, boxed)]
        optional_time: RefCell<Option<glib::DateTime>>,
        #[property(get, set, object, construct_only)]
        dummy: OnceCell<super::BaseObject>,
        #[property(get, set, object)]
        weak_obj: glib::WeakRef<glib::Object>,
        #[property(get, set, enum)]
        animal: Cell<super::Animal>,
        #[property(get, set, flags)]
        binding_flags: Cell<glib::BindingFlags>,
        #[property(get, set, builder_defaults = "[glib::ParamSpecObject::static_type()]")]
        pspec: RefCell<glib::ParamSpec>,
        #[property(get, set, builder_defaults = "[glib::VariantTy::INT32]")]
        variant: RefCell<glib::Variant>,
        #[property(get, set, override_class = "super::BaseObject")]
        renamed_string: RefCell<String>,
    }

    impl Default for ComplexProps {
        fn default() -> Self {
            Self {
                object_type: Cell::new(glib::Object::static_type()),
                time: RefCell::new(glib::DateTime::from_utc(1970, 1, 1, 0, 0, 0.).unwrap()),
                optional_time: Default::default(),
                dummy: Default::default(),
                weak_obj: Default::default(),
                animal: Cell::new(super::Animal::Dog),
                binding_flags: Cell::new(glib::BindingFlags::empty()),
                pspec: RefCell::new(Self::properties()[4].clone()),
                variant: RefCell::new(1i32.to_variant()),
                renamed_string: Default::default(),
            }
        }
    }
    impl super::BaseObjectImpl for ComplexProps {}
}

#[test]
fn complex_properties() {
    let dummy = glib::Object::new::<SmallObject>(&[]).unwrap();
    let obj = glib::Object::new::<ComplexProps>(&[("dummy", &dummy)]).unwrap();
    obj.set_renamed_string("hello".into());
    assert_eq!(&*obj.dummy().renamed_string(), "foobar");
    assert!(obj.weak_obj().is_none());
    {
        let weak = glib::Object::new::<SmallObject>(&[]).unwrap();
        obj.set_weak_obj(Some(weak.clone().upcast()));
        assert!(obj.weak_obj().is_some());
    }
    assert!(obj.weak_obj().is_none());
}

#[gobject::class(final)]
mod my_obj {
    use glib::StaticType;
    use std::cell::Cell;

    pub struct MyObj {
        #[property(get, set, builder(is_a_type("glib::Object::static_type()")))]
        object_type: Cell<glib::Type>,
    }
    impl Default for MyObj {
        fn default() -> Self {
            Self {
                object_type: Cell::new(glib::Object::static_type()),
            }
        }
    }
}

#[test]
#[should_panic(expected = "property 'object-type' of type 'MyObj' can't be set from given value")]
fn validation() {
    let obj = glib::Object::new::<MyObj>(&[]).unwrap();
    obj.set_object_type(glib::Type::U8);
}

#[gobject::class(final)]
mod pod {
    use std::cell::{Cell, RefCell};

    #[properties(pod)]
    #[derive(Default)]
    pub struct Pod {
        int_prop: Cell<i32>,
        string_prop: RefCell<String>,

        #[property(skip)]
        pub(super) skipped_field: Vec<(i32, bool)>,
    }
}

#[test]
fn pod_type() {
    use glib::subclass::prelude::*;

    let obj = glib::Object::new::<Pod>(&[]).unwrap();
    assert_eq!(obj.list_properties().len(), 2);
    assert!(obj.imp().skipped_field.is_empty());
    obj.set_int_prop(5);
    obj.set_string_prop("123".into());
}

#[derive(Clone, Debug, PartialEq, glib::Boxed)]
#[boxed_type(name = "Point", nullable)]
pub struct Point {
    x: f64,
    y: f64,
}

#[gobject::class]
mod optionals {
    use std::cell::RefCell;
    #[derive(Default)]
    pub struct Optionals {
        #[property(get, set, borrow, object)]
        obj: RefCell<Option<super::Pod>>,
        #[property(get, set, borrow, boxed)]
        point: RefCell<Option<super::Point>>,
    }
}

#[test]
fn optional_props() {
    let obj = glib::Object::new::<Optionals>(&[]).unwrap();

    assert!(obj.obj().is_none());
    obj.set_obj(Some(glib::Object::new::<Pod>(&[]).unwrap()));
    assert!(obj.obj().is_some());
    obj.set_obj(None);
    assert!(obj.obj().is_none());

    assert!(obj.point().is_none());
    let point = Point { x: 100., y: 200. };
    obj.set_point(Some(point.clone()));
    assert_eq!(obj.point().as_ref(), Some(&point));
    obj.set_point(None);
    assert!(obj.point().is_none());
}
