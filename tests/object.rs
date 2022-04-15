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
}

#[test]
fn object_derivable() {
    let obj = glib::Object::new::<ObjDerivable>(&[]).unwrap();
    obj.set_my_prop(52);
    ObjDerivableExt::set_my_prop(&obj, 53);
    obj.emit_abc();
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

#[gobject::class(final)]
mod obj_threadsafe {
    #[derive(Default)]
    struct ObjThreadSafe {
        #[property(get, set)]
        the_uint: std::sync::Mutex<u64>,
        #[property(get, set)]
        the_string: std::sync::RwLock<String>,
    }
    unsafe impl Send for ObjThreadSafe {}
    unsafe impl Sync for ObjThreadSafe {}
    impl ObjThreadSafe {
        #[signal]
        fn abc(&self) {}
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
    })
    .join()
    .unwrap();
    assert_eq!(obj.the_uint(), 256);
    assert_eq!(obj.the_string(), "Hello");
    assert!(flag.load(Ordering::Acquire));
}
