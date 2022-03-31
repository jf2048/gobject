#[gobject::interface]
mod iface {
    use std::marker::PhantomData;
    #[properties]
    #[derive(Copy, Clone)]
    pub struct Dummy {
        _parent: glib::gobject_ffi::GTypeInterface,
        #[property(get, set)]
        _my_prop: PhantomData<u64>,
    }
    #[methods]
    impl Dummy {
        #[signal]
        fn my_sig(iface: &super::Dummy, hello: i32) {}
    }
}

#[gobject::class(final, implements(Dummy))]
mod implement {
    use std::cell::Cell;
    #[properties]
    #[derive(Default)]
    pub struct Implementor {
        #[property(get, set, override_iface = "super::Dummy")]
        my_prop: Cell<u64>,
        #[property(get, set, explicit_notify, lax_validation,
            builder(minimum = -10, maximum = 10))]
        my_auto_prop: Cell<i64>,
    }
    impl super::DummyImpl for Implementor {}
}

#[test]
fn interface() {
    let obj = glib::Object::new::<Implementor>(&[]).unwrap();
    obj.set_my_prop(4000);
    obj.set_my_auto_prop(-5);
    obj.emit_my_sig(123);
}
