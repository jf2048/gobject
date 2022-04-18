#[gobject::class(final)]
mod obj_final {
    #[derive(Default)]
    struct ObjFinal {
        #[property(get, set)]
        my_prop: std::cell::Cell<u64>,
    }
    impl ObjFinal {
        #[signal]
        fn abc(&self) {}
    }
}

#[test]
fn object_final() {
    let obj = glib::Object::new::<ObjFinal>(&[]).unwrap();
    obj.set_my_prop(52);
    obj.emit_abc();
}

#[gobject::class]
mod obj_derivable {
    #[derive(Default)]
    pub struct ObjDerivable {
        #[property(get, set)]
        my_prop: std::cell::Cell<u64>,
    }
    impl ObjDerivable {
        #[signal]
        fn abc(&self) {}
    }
    impl super::ObjDerivable {
        #[constructor]
        pub fn new(my_prop: u64) -> Self {}
        #[constructor]
        pub fn with_prop_plus_one(my_prop: u64) -> Self {
            Self::new(my_prop + 1)
        }
    }
}

#[test]
fn object_derivable() {
    let obj = ObjDerivable::new(22);
    assert_eq!(obj.my_prop(), 22);
    obj.set_my_prop(52);
    assert_eq!(obj.my_prop(), 52);
    ObjDerivableExt::set_my_prop(&obj, 53);
    assert_eq!(obj.my_prop(), 53);
    obj.emit_abc();

    let obj = ObjDerivable::with_prop_plus_one(99);
    assert_eq!(obj.my_prop(), 100);
}

#[gobject::class]
mod obj_inner {
    #[derive(Default)]
    pub struct ObjInner {
        #[property(get, set)]
        my_prop: std::cell::Cell<u64>,
        my_uint: std::cell::Cell<u32>,
    }
    impl ObjInner {
        #[signal]
        fn abc(&self) {}
        fn properties() -> Vec<glib::ParamSpec> {
            vec![glib::ParamSpecUInt::new(
                "my-uint",
                "my-uint",
                "my-uint",
                0,
                u32::MAX,
                0,
                glib::ParamFlags::READWRITE,
            )]
        }

        fn set_property(
            &self,
            _obj: &super::ObjInner,
            _id: usize,
            value: &glib::Value,
            pspec: &glib::ParamSpec,
        ) {
            match pspec.name() {
                "my-uint" => self.my_uint.set(value.get().unwrap()),
                _ => unimplemented!(),
            }
        }

        fn property(
            &self,
            _obj: &super::ObjInner,
            _id: usize,
            pspec: &glib::ParamSpec,
        ) -> glib::Value {
            match pspec.name() {
                "my-uint" => glib::ToValue::to_value(&self.my_uint.get()),
                _ => unimplemented!(),
            }
        }

        fn signals() -> Vec<glib::subclass::Signal> {
            vec![glib::subclass::Signal::builder("xyz", &[], glib::Type::UNIT.into()).build()]
        }
    }
}

#[test]
fn object_inner_methods() {
    use glib::prelude::*;

    let obj = glib::Object::new::<ObjInner>(&[]).unwrap();
    assert_eq!(obj.list_properties().len(), 2);
    obj.emit_abc();
    obj.emit_by_name::<()>("xyz", &[]);
    obj.set_my_prop(22);
    obj.set_property("my-uint", 500u32);
    assert_eq!(obj.my_prop(), 22);
    assert_eq!(obj.property::<u32>("my-uint"), 500);
}

#[gobject::class(final, sync)]
mod obj_threadsafe {
    #[derive(Default)]
    struct ObjThreadSafe {
        #[property(get, set)]
        the_uint: std::sync::Mutex<u64>,
        #[property(get, set)]
        the_string: std::sync::RwLock<String>,
    }
    impl ObjThreadSafe {
        #[signal]
        fn abc(&self) {}
        #[public]
        async fn async_method(&self) {
            glib::timeout_future_seconds(0).await;
        }
    }
}

#[test]
fn concurrency() {
    use std::sync::atomic::{AtomicBool, Ordering};

    let obj = glib::Object::new::<ObjThreadSafe>(&[]).unwrap();
    let flag = std::sync::Arc::new(AtomicBool::new(false));
    let f = flag.clone();
    obj.connect_abc(move |_| {
        f.store(true, Ordering::Release);
    });
    let o = obj.clone();
    std::thread::spawn(move || {
        o.set_the_uint(256);
        o.set_the_string("Hello".into());
        o.emit_abc();
        glib::MainContext::default().block_on(o.async_method());
    })
    .join()
    .unwrap();
    assert_eq!(obj.the_uint(), 256);
    assert_eq!(obj.the_string(), "Hello");
    assert!(flag.load(Ordering::Acquire));
}

#[allow(unused_imports)]
mod objects {
    #[gobject::class(final)]
    mod obj_vis1 {
        #[derive(Default)]
        pub struct ObjVis1 {}
    }
    #[gobject::class(final)]
    mod obj_vis2 {
        #[derive(Default)]
        pub(crate) struct ObjVis2 {}
    }
    #[gobject::class]
    mod obj_vis3 {
        #[derive(Default)]
        struct ObjVis3 {}
    }
    #[gobject::interface]
    pub(super) mod iface_vis {
        impl IfaceVis {}
    }
    #[gobject::class(implements(IfaceVis))]
    mod obj_vis4 {
        #[derive(Default)]
        pub(in super::super) struct ObjVis4 {}
        impl super::IfaceVisImpl for ObjVis4 {}
    }
}

#[test]
fn visibility() {
    glib::Object::new::<objects::ObjVis1>(&[]).unwrap();
    glib::Object::new::<objects::ObjVis2>(&[]).unwrap();
    let obj = glib::Object::new::<objects::ObjVis4>(&[]).unwrap();
    glib::Cast::upcast::<objects::IfaceVis>(obj);
}

#[gobject::class]
mod public_methods {
    use glib::subclass::prelude::ObjectSubclassIsExt;

    #[derive(Default)]
    pub struct PublicMethods {
        #[property(get, set)]
        number: std::cell::Cell<u64>,
        #[property(get, set)]
        string: std::cell::RefCell<String>,
    }
    impl PublicMethods {
        #[public]
        pub fn say_hello() -> String {
            "hello".to_string()
        }
        #[public]
        pub fn get_number(&self) -> u64 {
            self.number.get()
        }
        #[public]
        pub fn get_string(#[is_a] obj: &super::PublicMethods) -> String {
            obj.imp().string.borrow().clone()
        }
        #[public]
        async fn get_string_async(&self) -> String {
            glib::timeout_future_seconds(0).await;
            self.string.borrow().clone()
        }
    }
    impl super::PublicMethods {
        #[public]
        pub fn say_goodbye() -> String {
            "goodbye".to_string()
        }
        #[public]
        pub fn get_number2(&self) -> u64 {
            self.imp().number.get()
        }
        #[public]
        pub fn get_string2(obj: &Self) -> String {
            obj.imp().string.borrow().clone()
        }
        #[constructor]
        pub fn new(number: u64, string: &str) -> Self {}
        #[constructor]
        pub fn default() -> Self {
            Self::new(100, "100")
        }
        #[public]
        async fn get_string2_async(&self) -> String {
            glib::timeout_future_seconds(0).await;
            self.imp().string.borrow().clone()
        }
    }
}

#[gobject::class(final, extends(PublicMethods))]
mod public_methods_final {
    use super::PublicMethodsExt;
    use glib::subclass::prelude::ObjectSubclassExt;
    use glib::Cast;

    #[derive(Default)]
    pub struct PublicMethodsFinal {}
    impl PublicMethodsFinal {
        #[public]
        pub fn say_hello2() -> String {
            "hello".to_string()
        }
        #[public]
        pub fn get_number3(&self) -> u64 {
            self.instance().get_number()
        }
        #[public]
        pub fn get_string3(obj: &super::PublicMethodsFinal) -> String {
            super::PublicMethods::get_string(obj)
        }
    }
    impl super::PublicMethodsFinal {
        #[public]
        pub fn say_goodbye2() -> String {
            "goodbye".to_string()
        }
        #[public]
        pub fn get_number4(&self) -> u64 {
            self.get_number2()
        }
        #[public]
        pub fn get_string4(obj: &Self) -> String {
            super::PublicMethods::get_string2(obj.upcast_ref())
        }
        #[constructor]
        pub fn new(number: u64, string: &str) -> Self {}
        #[constructor]
        pub fn default() -> Self {
            Self::new(100, "100")
        }
    }
    impl super::PublicMethodsImpl for PublicMethodsFinal {}
}
