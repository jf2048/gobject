#![cfg(feature = "use_gio")]

#[derive(Debug, Eq, PartialEq, Clone, Copy, glib::ErrorDomain)]
#[error_domain(name = "MyError")]
enum MyError {
    BadString,
}

#[gobject::class(final)]
mod my_initable {
    #[derive(Default)]
    struct MyInitable {
        #[property(get, set, builder(minimum = 0, maximum = 100))]
        my_prop: std::cell::Cell<u64>,
        #[property(get, set)]
        another_prop: std::cell::RefCell<String>,
    }
    impl MyInitable {
        fn init(&self) -> Result<(), glib::Error> {
            if *self.another_prop.borrow() == "bad string" {
                return Err(glib::Error::new(
                    super::MyError::BadString,
                    "got a bad string",
                ));
            }
            Ok(())
        }
        #[constructor(name = "new")]
        fn _new(my_prop: u64, another_prop: &str) -> Result<super::MyInitable, glib::BoolError> {}
        #[constructor(infallible)]
        fn new_infallible(my_prop: u64, another_prop: &str) -> super::MyInitable {}
    }
}

#[test]
fn initable() {
    let obj = MyInitable::new(50, "hello").unwrap();
    assert_eq!(obj.my_prop(), 50);
    assert_eq!(obj.another_prop(), "hello");
    assert!(MyInitable::new(200, "hello").is_err());
    assert!(MyInitable::new(50, "bad string").is_err());
    let obj = MyInitable::new_infallible(20, "good string");
    assert_eq!(obj.my_prop(), 20);
    assert_eq!(obj.another_prop(), "good string");
}

#[test]
#[should_panic(expected = "property 'my-prop' of type 'MyInitable' can't be set from given value")]
fn infallible_constructor() {
    MyInitable::new_infallible(120, "good string");
}
