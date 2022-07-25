#[gobject::interface(sync)]
mod iface {
    use std::marker::PhantomData;
    #[derive(Copy, Clone)]
    pub struct Dummy {
        #[property(get, set)]
        _my_prop: PhantomData<u64>,
    }
    impl Dummy {
        #[signal]
        fn my_sig(iface: &super::Dummy, hello: u64) {
            iface.set_my_prop(hello);
        }
        #[virt]
        fn my_virt(iface: &super::Dummy, #[is_a] _ignore: &glib::Object) -> u64 {
            iface.my_prop() + 100
        }
    }
    impl super::Dummy {
        #[public]
        pub fn dosomething(&self) -> String {
            format!("{}", self.my_prop())
        }
        #[virt]
        fn my_virt2(&self, v: u64) -> u64 {
            self.my_prop() + v
        }
    }
}

#[gobject::class(final, implements(Dummy))]
mod implement {
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct Implementor {
        #[property(get, set, override_iface = "super::Dummy")]
        my_prop: Mutex<u64>,
        #[property(get, set, explicit_notify, lax_validation,
            builder(minimum = -10, maximum = 10))]
        my_auto_prop: Mutex<i64>,
    }
    impl super::DummyImpl for Implementor {}
}

#[gobject::class(final, implements(Dummy))]
mod implement2 {
    use super::{DummyExt, DummyImplExt};
    use std::sync::Mutex;

    #[derive(Default)]
    pub struct Implementor2 {
        #[property(get, set, override_iface = "super::Dummy")]
        my_prop: Mutex<u64>,
    }
    impl Implementor2 {
        #[signal(override)]
        fn my_sig(&self, hello: u64) {
            self.parent_my_sig(55);
            assert_eq!(*self.my_prop.lock().unwrap(), 55);
            let mut v = self.my_prop.lock().unwrap();
            *v = hello + 22;
        }
    }
    impl super::DummyImpl for Implementor2 {
        fn my_virt(&self, obj: &Self::Type, _ignore: &glib::Object) -> u64 {
            obj.my_prop() + 200 + self.parent_my_virt(obj, _ignore)
        }
        fn my_virt2(&self, obj: &Self::Type, v: u64) -> u64 {
            self.parent_my_virt2(obj, v) + 1
        }
    }
}

#[test]
fn interface() {
    use std::sync::Arc;
    use std::sync::Mutex;

    let obj = glib::Object::new::<Implementor>(&[]).unwrap();
    obj.set_my_prop(4000);
    obj.set_my_auto_prop(-5);
    obj.emit_my_sig(123);
    assert_eq!(obj.my_prop(), 123);
    assert_eq!(obj.my_virt(&obj), 223);
    assert_eq!(obj.my_virt2(1), 124);

    let obj = glib::Object::new::<Implementor2>(&[]).unwrap();
    obj.emit_my_sig(133);
    assert_eq!(obj.my_prop(), 155);
    assert_eq!(obj.my_virt(&obj), 610);
    assert_eq!(obj.dosomething(), "155");
    assert_eq!(obj.my_virt2(1), 157);

    let called_signals: Arc<Mutex<Vec<String>>> = Default::default();
    obj.connect_my_sig(
        glib::clone!(@strong called_signals => move |_obj: &Implementor2, v: u64| {
            assert_eq!(v, 0);
            called_signals.lock().unwrap().push("my".to_owned());
        }),
    );
    std::thread::spawn(glib::clone!(@strong obj => move || {
        obj.emit_my_sig(0);
    }))
    .join()
    .unwrap();

    assert_eq!(*called_signals.lock().unwrap(), &["my"]);
}
