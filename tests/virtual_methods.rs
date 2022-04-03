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

#[test]
fn virtual_methods() {
    let obj = glib::Object::new::<Implementor>(&[]).unwrap();
    obj.set_my_prop(4000);
    obj.set_my_auto_prop(-5);
    assert_eq!(obj.emit_abc(), 300);
    assert_eq!(
        obj.virtual_concat("Hello", "World"),
        "overridden: Hello World 4000"
    );
}
