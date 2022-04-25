#[gobject::class(abstract)]
mod obj_abstract {
    use glib::subclass::types::ObjectSubclassExt;

    #[derive(Default)]
    pub struct ObjAbstract {
        #[property(get, set, abstract)]
        my_prop: std::marker::PhantomData<u64>,
    }
    impl ObjAbstract {
        #[signal]
        fn abc(&self) -> i32 {
            100
        }
        #[virt]
        fn virtual_concat(&self, a: &str, b: &str) -> String {
            format!("{} {} {}", self.instance().my_prop(), a, b)
        }
        #[virt]
        fn virtual_method(&self, implementor: &super::Implementor) -> String {
            glib::ObjectExt::type_(implementor).name().to_owned()
        }
    }
}

#[gobject::class(final, extends(ObjAbstract))]
mod obj_implementor {
    use std::cell::Cell;
    #[derive(Default)]
    pub struct Implementor {
        #[property(get, set, override_class = "super::ObjAbstract")]
        my_prop: Cell<u64>,
        #[property(get, set, explicit_notify, lax_validation,
            builder(minimum = -10, maximum = 10))]
        my_auto_prop: Cell<i64>,
    }
    impl Implementor {
        #[signal(override)]
        fn abc(&self) -> i32 {
            200 + self.parent_abc()
        }
    }
    impl super::ObjAbstractImpl for Implementor {
        fn virtual_concat(&self, _obj: &Self::Type, a: &str, b: &str) -> String {
            format!("overridden: {} {} {}", a, b, self.my_prop.get())
        }
    }
}

#[gobject::class(extends(ObjAbstract), parent_trait = "super::ObjAbstractImpl")]
mod obj_derivable {
    use super::ObjAbstractImplExt;
    use std::cell::Cell;
    #[derive(Default)]
    pub struct ObjDerivable {
        #[property(get, set, override_class = "super::ObjAbstract")]
        my_prop: Cell<u64>,
    }
    impl ObjDerivable {
        #[virt]
        fn another_virtual(&self) {
            self.my_prop.set(1000);
        }
    }
    impl super::ObjAbstractImpl for ObjDerivable {
        fn virtual_concat(&self, obj: &Self::Type, a: &str, b: &str) -> String {
            format!("({})", self.parent_virtual_concat(obj, a, b))
        }
    }
}

#[gobject::class(final, extends(ObjDerivable, ObjAbstract))]
mod obj_implementor2 {
    use super::ObjAbstractExt;
    use super::ObjAbstractImplExt;
    use super::ObjDerivableImplExt;
    #[derive(Default)]
    pub struct Implementor2 {}
    impl Implementor2 {
        #[signal(override)]
        fn abc(&self) -> i32 {
            300 + self.parent_abc()
        }
    }
    impl super::ObjAbstractImpl for Implementor2 {
        fn virtual_concat(&self, obj: &Self::Type, a: &str, b: &str) -> String {
            format!(
                "overridden again: {}",
                self.parent_virtual_concat(obj, b, a)
            )
        }
    }
    impl super::ObjDerivableImpl for Implementor2 {
        fn another_virtual(&self, obj: &Self::Type) {
            self.parent_another_virtual(obj);
            assert_eq!(obj.my_prop(), 1000);
            obj.set_my_prop(2000);
        }
    }
}

#[test]
fn virtual_methods() {
    let obj = glib::Object::new::<Implementor>(&[]).unwrap();
    obj.set_my_prop(9000);
    obj.set_my_auto_prop(-5);
    assert_eq!(obj.emit_abc(), 300);
    assert_eq!(
        obj.virtual_concat("Hello", "World"),
        "overridden: Hello World 9000"
    );

    let d = glib::Object::new::<ObjDerivable>(&[]).unwrap();
    assert_eq!(d.emit_abc(), 100);
    assert_eq!(d.my_prop(), 0);
    d.another_virtual();
    assert_eq!(d.my_prop(), 1000);
    assert_eq!(d.virtual_concat("Hello", "World"), "(1000 Hello World)");

    let i2 = glib::Object::new::<Implementor2>(&[]).unwrap();
    assert_eq!(i2.emit_abc(), 400);
    i2.another_virtual();
    assert_eq!(i2.my_prop(), 2000);
    assert_eq!(
        i2.virtual_concat("Hello", "World"),
        "overridden again: (2000 World Hello)"
    );
}
