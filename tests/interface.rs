#[gobject::interface]
mod iface {
    use std::marker::PhantomData;
    #[derive(Copy, Clone)]
    pub struct Dummy {
        _parent: glib::gobject_ffi::GTypeInterface,
        #[property(get, set)]
        _my_prop: PhantomData<u64>,
    }
    impl Dummy {
        #[signal]
        fn my_sig(iface: &super::Dummy, hello: u64) {
            iface.set_my_prop(hello);
        }
        #[virt]
        fn my_virt(iface: &super::Dummy) -> u64 {
            iface.my_prop() + 100
        }
    }
}

#[gobject::class(final, implements(Dummy))]
mod implement {
    use std::cell::Cell;
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

#[gobject::class(final, implements(Dummy))]
mod implement2 {
    use std::cell::Cell;
    #[derive(Default)]
    pub struct Implementor2 {
        #[property(get, set, override_iface = "super::Dummy")]
        my_prop: Cell<u64>,
    }
    impl Implementor2 {
        #[signal(override)]
        fn my_sig(&self, hello: u64) {
            self.parent_my_sig(55);
            assert_eq!(self.my_prop.get(), 55);
            self.my_prop.set(hello + 22);
        }
    }
    impl super::DummyImpl for Implementor2 {
        fn my_virt(self, iface: &Self::Type) -> u64 {
            iface.my_prop() + 200
        }
    }
}

#[test]
fn interface() {
    let obj = glib::Object::new::<Implementor>(&[]).unwrap();
    obj.set_my_prop(4000);
    obj.set_my_auto_prop(-5);
    obj.emit_my_sig(123);
}
