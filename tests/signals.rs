#[gobject::class]
mod obj_signals {
    use std::cell::RefCell;
    use std::ops::ControlFlow;

    #[derive(Default)]
    pub struct Signals {
        pub(super) log: RefCell<Vec<String>>,
    }
    impl Signals {
        pub(super) fn append(&self, msg: &str) {
            self.log.borrow_mut().push(msg.to_owned());
        }
        #[signal]
        fn noparam(&self) {}
        #[signal]
        fn param(&self, hello: i32) {}
        #[signal]
        fn twoparams(&self, hello: i32, world: String) {}
        #[signal(run_last)]
        fn with_handler(&self, _hello: i32, world: String) {
            self.append(&(world + " last"));
        }
        #[signal(run_first)]
        fn with_retval(&self, val: i32) -> i32 {
            val + 5
        }
        #[signal(run_first)]
        fn with_accumulator(&self, val: i32) -> i32 {
            val + 10
        }
        #[accumulator(signal = "with-accumulator")]
        fn with_accumulator_acc(accu: i32, val: i32) -> ControlFlow<Option<i32>, Option<i32>> {
            ControlFlow::Continue(Some(accu + val))
        }
        #[signal(detailed, run_cleanup)]
        fn has_detail(&self, val: u32) -> u32 {
            val + 7
        }
        #[accumulator(signal = "has-detail")]
        fn has_detail_acc(
            hint: &glib::subclass::signal::SignalInvocationHint,
            mut accu: u32,
            val: u32,
        ) -> ControlFlow<Option<u32>, Option<u32>> {
            if let Some(quark) = hint.detail() {
                if quark.as_str() == "hello" {
                    accu += 100;
                }
            }
            accu += val;
            ControlFlow::Continue(Some(accu))
        }
        #[signal(run_first)]
        fn string_appender(&self, s: &str) -> String {
            format!("class({})", s)
        }
        #[accumulator(signal = "string-appender")]
        fn string_appender_acc(
            accu: Option<&str>,
            val: &str,
        ) -> ControlFlow<Option<String>, Option<String>> {
            let new = accu
                .map(|a| format!("{}, add({})", a, val))
                .unwrap_or_else(|| format!("first({})", val));
            ControlFlow::Continue(Some(new))
        }
    }
}

#[test]
fn signals() {
    use glib::subclass::prelude::*;

    let signals = glib::Object::new::<Signals>(&[]).unwrap();

    signals.emit_noparam();
    signals.connect_noparam(|sig| {
        sig.imp().append("noparam");
    });
    signals.emit_noparam();

    signals.connect_with_handler(|sig, hello, world| {
        assert_eq!(hello, 500);
        sig.imp().append(&world);
    });
    signals.emit_with_handler(500, "handler".into());

    assert_eq!(
        *signals.imp().log.borrow(),
        &["noparam", "handler", "handler last"]
    );

    assert_eq!(signals.emit_with_retval(10), 15);
    signals.connect_with_retval(|_, val| val * 2);
    assert_eq!(signals.emit_with_retval(10), 20);

    assert_eq!(signals.emit_with_accumulator(10), 20);
    signals.connect_with_accumulator(|_, val| val * 3);
    assert_eq!(signals.emit_with_accumulator(10), 50);

    assert_eq!(signals.emit_has_detail(None, 10), 17);
    assert_eq!(signals.emit_has_detail(Some("hello".into()), 10), 117);
    signals.connect_has_detail(Some("hello".into()), |_, val| val * 3);
    assert_eq!(signals.emit_has_detail(None, 20), 27);
    assert_eq!(signals.emit_has_detail(Some("hello".into()), 20), 287);

    assert_eq!(signals.emit_string_appender("a"), "first(class(a))");
    signals.connect_string_appender(|_, val| format!("closure({})", val));
    assert_eq!(
        signals.emit_string_appender("b"),
        "first(class(b)), add(closure(b))"
    );
}
