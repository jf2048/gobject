#![cfg(feature = "use_gio")]

#[gobject::class(
    extends(gio::Application),
    parent_trait = "gio::subclass::prelude::ApplicationImpl",
    inherits(gio::ActionGroup, gio::ActionMap)
)]
mod app {
    use gio::prelude::*;
    use gio::subclass::prelude::*;
    use glib::Variant;

    #[derive(Default)]
    struct MyApp {
        log: std::cell::RefCell<Vec<String>>,
    }
    impl MyApp {
        fn log(&self, msg: &str) {
            self.log.borrow_mut().push(msg.to_owned());
        }
        #[action]
        fn action1() {
            super::MyApp::default().imp().log("action1");
        }
        #[action(name = "renamed-action2")]
        fn action2() {
            super::MyApp::default().imp().log("action2");
        }
        #[action(disabled)]
        fn action3(value: i32) {
            let app = super::MyApp::default();
            app.imp().log(&format!("action3 {}", value));
            app.lookup_action("action3")
                .unwrap()
                .downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(false);
        }
        #[action]
        fn action4(&self) {
            self.log("action4");
        }
        #[action]
        fn action5(&self, value: i32) {
            self.log(&format!("action5 {}", value));
        }
        #[action]
        fn action6(&self, value: i32, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "action6");
            self.log(&format!("action6 {}", value));
            action.set_enabled(false);
        }
        #[action]
        fn action7(&self, value: i32, #[action] action: &gio::Action) {
            assert_eq!(action.name(), "action7");
            self.log(&format!("action7 {}", value));
        }
        #[action]
        fn action8(&self, value: i32, #[action] action: &gio::SimpleAction, #[state] state: u32) {
            assert_eq!(state, action.state().unwrap().get::<u32>().unwrap());
            assert_eq!(action.name(), "action8");
            self.log(&format!("action8 {}", value));
        }
        #[action(change_state, name = "action8")]
        fn action8_change_state(&self, value: u32) {
            self.log(&format!("action8 change {}", value));
        }
        #[action(default = "100i32", hint = "(0i32, 100i32)")]
        fn action9(&self, #[state] state: i32) {
            assert_eq!(state, 100i32);
            self.log("action9");
        }
        #[action(
            default_variant = "100i32.to_variant()",
            hint = "(0i32, 100i32).to_variant()"
        )]
        fn action10(&self, value: i32, #[state] state: Variant) {
            assert_eq!(state.get::<i32>().unwrap(), 100i32);
            self.log(&format!("action10 {}", value));
        }
        #[action(parameter_type_str = "(ii)")]
        fn action11(&self, value: Variant) {
            assert_eq!(value.type_(), <(i32, i32)>::static_variant_type());
            self.log(&format!(
                "action11 {:?}",
                value.get::<(i32, i32)>().unwrap()
            ));
        }
        #[action]
        fn action12(&self, value: i32) -> Option<i32> {
            self.log(&format!("action12 {}", value));
            Some(value)
        }
        #[action(change_state, name = "action12")]
        fn action12_change_state(&self, value: i32) -> Option<i32> {
            self.log(&format!("action12 change {}", value));
            Some(value)
        }
        #[action(parameter_type_str = "i", default_variant = "0i32.to_variant()")]
        fn action13(&self, value: Variant) -> Option<Variant> {
            self.log(&format!("action13 {}", value));
            Some(value)
        }
        #[action(change_state, name = "action13")]
        fn action13_change_state(&self, value: Variant) -> Option<Variant> {
            self.log(&format!("action13 change {}", value));
            Some(value)
        }
        #[action]
        async fn action14(&self) {
            glib::timeout_future_seconds(0).await;
            self.log("action14");
        }
        #[action]
        async fn action15(&self, value: String, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "action15");
            glib::timeout_future_seconds(0).await;
            self.log(&format!("action15 {}", value));
        }
        #[action]
        #[public]
        fn pub_action1() {
            super::MyApp::default().imp().log("pub-action1");
        }
        #[action(name = "renamed-pub-action2")]
        #[public]
        fn pub_action2() {
            super::MyApp::default().imp().log("pub-action2");
        }
        #[action(disabled)]
        #[public]
        fn pub_action3(value: i32) {
            let app = super::MyApp::default();
            app.imp().log(&format!("pub-action3 {}", value));
            app.lookup_action("pub-action3")
                .unwrap()
                .downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(false);
        }
        #[action]
        #[public]
        fn pub_action4(&self) {
            self.log("pub-action4");
        }
        #[action]
        #[public]
        fn pub_action5(&self, value: i32) {
            self.log(&format!("pub-action5 {}", value));
        }
        #[action]
        #[public]
        fn pub_action6(&self, value: i32, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "pub-action6");
            self.log(&format!("pub-action6 {}", value));
            action.set_enabled(false);
        }
        #[action]
        #[public]
        fn pub_action7(&self, value: i32, #[action] action: &gio::Action) {
            assert_eq!(action.name(), "pub-action7");
            self.log(&format!("pub-action7 {}", value));
        }
        #[action]
        #[public]
        fn pub_action8(
            &self,
            value: i32,
            #[action] action: &gio::SimpleAction,
            #[state] state: u32,
        ) {
            assert_eq!(state, action.state().unwrap().get::<u32>().unwrap());
            assert_eq!(action.name(), "pub-action8");
            self.log(&format!("pub-action8 {}", value));
        }
        #[action(change_state, name = "pub-action8")]
        #[public]
        fn pub_action8_change_state(&self, value: u32) {
            self.log(&format!("pub-action8 change {}", value));
        }
        #[action(default = "100i32", hint = "(0i32, 100i32)")]
        #[public]
        fn pub_action9(&self, #[state] state: i32) {
            assert_eq!(state, 100i32);
            self.log("pub-action9");
        }
        #[action(
            default_variant = "100i32.to_variant()",
            hint = "(0i32, 100i32).to_variant()"
        )]
        #[public]
        fn pub_action10(&self, value: i32, #[state] state: Variant) {
            assert_eq!(state.get::<i32>().unwrap(), 100i32);
            self.log(&format!("pub-action10 {}", value));
        }
        #[action(parameter_type_str = "(ii)")]
        #[public]
        fn pub_action11(&self, value: Variant) {
            assert_eq!(value.type_(), <(i32, i32)>::static_variant_type());
            self.log(&format!(
                "pub-action11 {:?}",
                value.get::<(i32, i32)>().unwrap()
            ));
        }
        #[action]
        #[public]
        fn pub_action12(&self, value: i32) -> Option<i32> {
            self.log(&format!("pub-action12 {}", value));
            Some(value)
        }
        #[action(change_state, name = "pub-action12")]
        #[public]
        fn pub_action12_change_state(&self, value: i32) -> Option<i32> {
            self.log(&format!("pub-action12 change {}", value));
            Some(value)
        }
        #[action(parameter_type_str = "i", default_variant = "0i32.to_variant()")]
        #[public]
        fn pub_action13(&self, value: Variant) -> Option<Variant> {
            self.log(&format!("pub-action13 {}", value));
            Some(value)
        }
        #[action(change_state, name = "pub-action13")]
        #[public]
        fn pub_action13_change_state(&self, value: Variant) -> Option<Variant> {
            self.log(&format!("pub-action13 change {}", value));
            Some(value)
        }
        #[action]
        #[public]
        async fn pub_action14(&self) {
            glib::timeout_future_seconds(0).await;
            self.log("pub-action14");
        }
        #[action]
        #[public]
        async fn pub_action15(&self, value: String, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "pub-action15");
            glib::timeout_future_seconds(0).await;
            self.log(&format!("pub-action15 {}", value));
        }
    }
    impl super::MyApp {
        fn default() -> Self {
            gio::Application::default()
                .unwrap()
                .downcast::<super::MyApp>()
                .unwrap()
        }
        #[constructor(infallible)]
        pub fn new(application_id: &str) -> Self {}
        pub fn take_log(&self) -> Vec<String> {
            self.imp().log.take()
        }
        #[action]
        fn wrapper_action1() {
            super::MyApp::default().imp().log("wrapper-action1");
        }
        #[action(name = "renamed-wrapper-action2")]
        fn wrapper_action2() {
            super::MyApp::default().imp().log("wrapper-action2");
        }
        #[action(disabled)]
        fn wrapper_action3(value: i32) {
            let app = super::MyApp::default();
            app.imp().log(&format!("wrapper-action3 {}", value));
            app.lookup_action("wrapper-action3")
                .unwrap()
                .downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(false);
        }
        #[action]
        fn wrapper_action4(&self) {
            self.imp().log("wrapper-action4");
        }
        #[action]
        fn wrapper_action5(&self, value: i32) {
            self.imp().log(&format!("wrapper-action5 {}", value));
        }
        #[action]
        fn wrapper_action6(&self, value: i32, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "wrapper-action6");
            self.imp().log(&format!("wrapper-action6 {}", value));
            action.set_enabled(false);
        }
        #[action]
        fn wrapper_action7(&self, value: i32, #[action] action: &gio::Action) {
            assert_eq!(action.name(), "wrapper-action7");
            self.imp().log(&format!("wrapper-action7 {}", value));
        }
        #[action]
        fn wrapper_action8(
            &self,
            value: i32,
            #[action] action: &gio::SimpleAction,
            #[state] state: u32,
        ) {
            assert_eq!(state, action.state().unwrap().get::<u32>().unwrap());
            assert_eq!(action.name(), "wrapper-action8");
            self.imp().log(&format!("wrapper-action8 {}", value));
        }
        #[action(change_state, name = "wrapper-action8")]
        fn wrapper_action8_change_state(&self, value: u32) {
            self.imp().log(&format!("wrapper-action8 change {}", value));
        }
        #[action(default = "100i32", hint = "(0i32, 100i32)")]
        fn wrapper_action9(&self, #[state] state: i32) {
            assert_eq!(state, 100i32);
            self.imp().log("wrapper-action9");
        }
        #[action(
            default_variant = "100i32.to_variant()",
            hint = "(0i32, 100i32).to_variant()"
        )]
        fn wrapper_action10(&self, value: i32, #[state] state: Variant) {
            assert_eq!(state.get::<i32>().unwrap(), 100i32);
            self.imp().log(&format!("wrapper-action10 {}", value));
        }
        #[action(parameter_type_str = "(ii)")]
        fn wrapper_action11(&self, value: Variant) {
            assert_eq!(value.type_(), <(i32, i32)>::static_variant_type());
            self.imp().log(&format!(
                "wrapper-action11 {:?}",
                value.get::<(i32, i32)>().unwrap()
            ));
        }
        #[action]
        fn wrapper_action12(&self, value: i32) -> Option<i32> {
            self.imp().log(&format!("wrapper-action12 {}", value));
            Some(value)
        }
        #[action(change_state, name = "wrapper-action12")]
        fn wrapper_action12_change_state(&self, value: i32) -> Option<i32> {
            self.imp()
                .log(&format!("wrapper-action12 change {}", value));
            Some(value)
        }
        #[action(parameter_type_str = "i", default_variant = "0i32.to_variant()")]
        fn wrapper_action13(&self, value: Variant) -> Option<Variant> {
            self.imp().log(&format!("wrapper-action13 {}", value));
            Some(value)
        }
        #[action(change_state, name = "wrapper-action13")]
        fn wrapper_action13_change_state(&self, value: Variant) -> Option<Variant> {
            self.imp()
                .log(&format!("wrapper-action13 change {}", value));
            Some(value)
        }
        #[action]
        async fn wrapper_action14(&self) {
            glib::timeout_future_seconds(0).await;
            self.imp().log("wrapper-action14");
        }
        #[action]
        async fn wrapper_action15(&self, value: String, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "wrapper-action15");
            glib::timeout_future_seconds(0).await;
            self.imp().log(&format!("wrapper-action15 {}", value));
        }
        #[action]
        #[public]
        fn pub_wrapper_action1() {
            super::MyApp::default().imp().log("pub-wrapper-action1");
        }
        #[action(name = "renamed-pub-wrapper-action2")]
        #[public]
        fn pub_wrapper_action2() {
            super::MyApp::default().imp().log("pub-wrapper-action2");
        }
        #[action(disabled)]
        #[public]
        fn pub_wrapper_action3(value: i32) {
            let app = super::MyApp::default();
            app.imp().log(&format!("pub-wrapper-action3 {}", value));
            app.lookup_action("pub-wrapper-action3")
                .unwrap()
                .downcast::<gio::SimpleAction>()
                .unwrap()
                .set_enabled(false);
        }
        #[action]
        #[public]
        fn pub_wrapper_action4(&self) {
            self.imp().log("pub-wrapper-action4");
        }
        #[action]
        #[public]
        fn pub_wrapper_action5(&self, value: i32) {
            self.imp().log(&format!("pub-wrapper-action5 {}", value));
        }
        #[action]
        #[public]
        fn pub_wrapper_action6(&self, value: i32, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "pub-wrapper-action6");
            self.imp().log(&format!("pub-wrapper-action6 {}", value));
            action.set_enabled(false);
        }
        #[action]
        #[public]
        fn pub_wrapper_action7(&self, value: i32, #[action] action: &gio::Action) {
            assert_eq!(action.name(), "pub-wrapper-action7");
            self.imp().log(&format!("pub-wrapper-action7 {}", value));
        }
        #[action]
        #[public]
        fn pub_wrapper_action8(
            &self,
            value: i32,
            #[action] action: &gio::SimpleAction,
            #[state] state: u32,
        ) {
            assert_eq!(state, action.state().unwrap().get::<u32>().unwrap());
            assert_eq!(action.name(), "pub-wrapper-action8");
            self.imp().log(&format!("pub-wrapper-action8 {}", value));
        }
        #[action(change_state, name = "pub-wrapper-action8")]
        #[public]
        fn pub_wrapper_action8_change_state(&self, value: u32) {
            self.imp()
                .log(&format!("pub-wrapper-action8 change {}", value));
        }
        #[action(default = "100i32", hint = "(0i32, 100i32)")]
        #[public]
        fn pub_wrapper_action9(&self, #[state] state: i32) {
            assert_eq!(state, 100i32);
            self.imp().log("pub-wrapper-action9");
        }
        #[action(
            default_variant = "100i32.to_variant()",
            hint = "(0i32, 100i32).to_variant()"
        )]
        #[public]
        fn pub_wrapper_action10(&self, value: i32, #[state] state: Variant) {
            assert_eq!(state.get::<i32>().unwrap(), 100i32);
            self.imp().log(&format!("pub-wrapper-action10 {}", value));
        }
        #[action(parameter_type_str = "(ii)")]
        #[public]
        fn pub_wrapper_action11(&self, value: Variant) {
            assert_eq!(value.type_(), <(i32, i32)>::static_variant_type());
            self.imp().log(&format!(
                "pub-wrapper-action11 {:?}",
                value.get::<(i32, i32)>().unwrap()
            ));
        }
        #[action]
        #[public]
        fn pub_wrapper_action12(&self, value: i32) -> Option<i32> {
            self.imp().log(&format!("pub-wrapper-action12 {}", value));
            Some(value)
        }
        #[action(change_state, name = "pub-wrapper-action12")]
        #[public]
        fn pub_wrapper_action12_change_state(&self, value: i32) -> Option<i32> {
            self.imp()
                .log(&format!("pub-wrapper-action12 change {}", value));
            Some(value)
        }
        #[action(parameter_type_str = "i", default_variant = "0i32.to_variant()")]
        #[public]
        fn pub_wrapper_action13(&self, value: Variant) -> Option<Variant> {
            self.imp().log(&format!("pub-wrapper-action13 {}", value));
            Some(value)
        }
        #[action(change_state, name = "pub-wrapper-action13")]
        #[public]
        fn pub_wrapper_action13_change_state(&self, value: Variant) -> Option<Variant> {
            self.imp()
                .log(&format!("pub-wrapper-action13 change {}", value));
            Some(value)
        }
        #[action]
        #[public]
        async fn pub_wrapper_action14(&self) {
            glib::timeout_future_seconds(0).await;
            self.imp().log("pub-wrapper-action14");
        }
        #[action]
        #[public]
        async fn pub_wrapper_action15(&self, value: String, #[action] action: &gio::SimpleAction) {
            assert_eq!(action.name(), "pub-wrapper-action15");
            glib::timeout_future_seconds(0).await;
            self.imp().log(&format!("pub-wrapper-action15 {}", value));
        }
    }
    impl ApplicationImpl for MyApp {}
}

#[test]
fn actions() {
    use gio::prelude::*;
    let app = MyApp::new("org.dummy.test");
    app.connect_activate(|app| {
        app.activate_action("action1", None);
        app.activate_action("renamed-action2", None);
        app.activate_action("action3", Some(&3i32.to_variant()));
        app.lookup_action("action3")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);
        app.activate_action("action3", Some(&3i32.to_variant()));
        app.activate_action("action4", None);
        app.activate_action("action5", Some(&5i32.to_variant()));
        app.activate_action("action6", Some(&6i32.to_variant()));
        app.activate_action("action6", Some(&6i32.to_variant()));
        app.activate_action("action7", Some(&7i32.to_variant()));
        app.activate_action("action8", Some(&8i32.to_variant()));
        app.change_action_state("action8", &80u32.to_variant());
        app.activate_action("action9", None);
        app.activate_action("action10", Some(&10i32.to_variant()));
        app.activate_action("action11", Some(&(11i32, 111i32).to_variant()));
        app.activate_action("action12", Some(&12i32.to_variant()));
        app.change_action_state("action12", &120i32.to_variant());
        app.activate_action("action13", Some(&13i32.to_variant()));
        app.change_action_state("action13", &130i32.to_variant());
        app.activate_action("action14", None);
        app.activate_action("action15", Some(&"Hello".to_variant()));
        glib::MainContext::default().block_on(async move {
            glib::timeout_future_seconds(0).await;
        });
        let log = app.take_log();
        assert_eq!(
            log,
            [
                "action1",
                "action2",
                "action3 3",
                "action4",
                "action5 5",
                "action6 6",
                "action7 7",
                "action8 8",
                "action8 change 80",
                "action9",
                "action10 10",
                "action11 (11, 111)",
                "action12 12",
                "action12 change 12",
                "action12 change 120",
                "action13 13",
                "action13 change 13",
                "action13 change 130",
                "action14",
                "action15 Hello",
            ]
        );

        app.activate_action("pub-action1", None);
        app.activate_action("renamed-pub-action2", None);
        app.activate_action("pub-action3", Some(&3i32.to_variant()));
        app.lookup_action("pub-action3")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);
        app.activate_action("pub-action3", Some(&3i32.to_variant()));
        app.activate_action("pub-action4", None);
        app.activate_action("pub-action5", Some(&5i32.to_variant()));
        app.activate_action("pub-action6", Some(&6i32.to_variant()));
        app.activate_action("pub-action6", Some(&6i32.to_variant()));
        app.activate_action("pub-action7", Some(&7i32.to_variant()));
        app.activate_action("pub-action8", Some(&8i32.to_variant()));
        app.change_action_state("pub-action8", &80u32.to_variant());
        app.activate_action("pub-action9", None);
        app.activate_action("pub-action10", Some(&10i32.to_variant()));
        app.activate_action("pub-action11", Some(&(11i32, 111i32).to_variant()));
        app.activate_action("pub-action12", Some(&12i32.to_variant()));
        app.change_action_state("pub-action12", &120i32.to_variant());
        app.activate_action("pub-action13", Some(&13i32.to_variant()));
        app.change_action_state("pub-action13", &130i32.to_variant());
        app.activate_action("pub-action14", None);
        app.activate_action("pub-action15", Some(&"Hello".to_variant()));
        glib::MainContext::default().block_on(async move {
            glib::timeout_future_seconds(0).await;
        });
        let log = app.take_log();
        assert_eq!(
            log,
            [
                "pub-action1",
                "pub-action2",
                "pub-action3 3",
                "pub-action4",
                "pub-action5 5",
                "pub-action6 6",
                "pub-action7 7",
                "pub-action8 8",
                "pub-action8 change 80",
                "pub-action9",
                "pub-action10 10",
                "pub-action11 (11, 111)",
                "pub-action12 12",
                "pub-action12 change 12",
                "pub-action12 change 120",
                "pub-action13 13",
                "pub-action13 change 13",
                "pub-action13 change 130",
                "pub-action14",
                "pub-action15 Hello",
            ]
        );

        app.lookup_action("pub-action6")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);

        app.pub_action1();
        app.pub_action2();
        app.pub_action3(3);
        app.lookup_action("pub-action3")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);
        app.pub_action3(3);
        app.pub_action4();
        app.pub_action5(5);
        app.pub_action6(6);
        app.pub_action6(6);
        app.pub_action7(7);
        app.pub_action8(8);
        app.pub_action8_change_state(80);
        app.pub_action9();
        app.pub_action10(10);
        app.pub_action11((11i32, 111i32).to_variant());
        app.pub_action12(12);
        app.pub_action12_change_state(120);
        app.pub_action13(13i32.to_variant());
        app.pub_action13_change_state(130i32.to_variant());
        glib::MainContext::default().block_on(async move {
            app.pub_action14().await;
            app.pub_action15("Hello".into()).await;
        });
        let log = app.take_log();
        assert_eq!(
            log,
            [
                "pub-action1",
                "pub-action2",
                "pub-action3 3",
                "pub-action4",
                "pub-action5 5",
                "pub-action6 6",
                "pub-action7 7",
                "pub-action8 8",
                "pub-action8 change 80",
                "pub-action9",
                "pub-action10 10",
                "pub-action11 (11, 111)",
                "pub-action12 12",
                "pub-action12 change 12",
                "pub-action12 change 120",
                "pub-action13 13",
                "pub-action13 change 13",
                "pub-action13 change 130",
                "pub-action14",
                "pub-action15 Hello",
            ]
        );

        app.activate_action("wrapper-action1", None);
        app.activate_action("renamed-wrapper-action2", None);
        app.activate_action("wrapper-action3", Some(&3i32.to_variant()));
        app.lookup_action("wrapper-action3")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);
        app.activate_action("wrapper-action3", Some(&3i32.to_variant()));
        app.activate_action("wrapper-action4", None);
        app.activate_action("wrapper-action5", Some(&5i32.to_variant()));
        app.activate_action("wrapper-action6", Some(&6i32.to_variant()));
        app.activate_action("wrapper-action6", Some(&6i32.to_variant()));
        app.activate_action("wrapper-action7", Some(&7i32.to_variant()));
        app.activate_action("wrapper-action8", Some(&8i32.to_variant()));
        app.change_action_state("wrapper-action8", &80u32.to_variant());
        app.activate_action("wrapper-action9", None);
        app.activate_action("wrapper-action10", Some(&10i32.to_variant()));
        app.activate_action("wrapper-action11", Some(&(11i32, 111i32).to_variant()));
        app.activate_action("wrapper-action12", Some(&12i32.to_variant()));
        app.change_action_state("wrapper-action12", &120i32.to_variant());
        app.activate_action("wrapper-action13", Some(&13i32.to_variant()));
        app.change_action_state("wrapper-action13", &130i32.to_variant());
        app.activate_action("wrapper-action14", None);
        app.activate_action("wrapper-action15", Some(&"Hello".to_variant()));
        glib::MainContext::default().block_on(async move {
            glib::timeout_future_seconds(0).await;
        });
        let log = app.take_log();
        assert_eq!(
            log,
            [
                "wrapper-action1",
                "wrapper-action2",
                "wrapper-action3 3",
                "wrapper-action4",
                "wrapper-action5 5",
                "wrapper-action6 6",
                "wrapper-action7 7",
                "wrapper-action8 8",
                "wrapper-action8 change 80",
                "wrapper-action9",
                "wrapper-action10 10",
                "wrapper-action11 (11, 111)",
                "wrapper-action12 12",
                "wrapper-action12 change 12",
                "wrapper-action12 change 120",
                "wrapper-action13 13",
                "wrapper-action13 change 13",
                "wrapper-action13 change 130",
                "wrapper-action14",
                "wrapper-action15 Hello",
            ]
        );

        app.activate_action("pub-wrapper-action1", None);
        app.activate_action("renamed-pub-wrapper-action2", None);
        app.activate_action("pub-wrapper-action3", Some(&3i32.to_variant()));
        app.lookup_action("pub-wrapper-action3")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);
        app.activate_action("pub-wrapper-action3", Some(&3i32.to_variant()));
        app.activate_action("pub-wrapper-action4", None);
        app.activate_action("pub-wrapper-action5", Some(&5i32.to_variant()));
        app.activate_action("pub-wrapper-action6", Some(&6i32.to_variant()));
        app.activate_action("pub-wrapper-action6", Some(&6i32.to_variant()));
        app.activate_action("pub-wrapper-action7", Some(&7i32.to_variant()));
        app.activate_action("pub-wrapper-action8", Some(&8i32.to_variant()));
        app.change_action_state("pub-wrapper-action8", &80u32.to_variant());
        app.activate_action("pub-wrapper-action9", None);
        app.activate_action("pub-wrapper-action10", Some(&10i32.to_variant()));
        app.activate_action("pub-wrapper-action11", Some(&(11i32, 111i32).to_variant()));
        app.activate_action("pub-wrapper-action12", Some(&12i32.to_variant()));
        app.change_action_state("pub-wrapper-action12", &120i32.to_variant());
        app.activate_action("pub-wrapper-action13", Some(&13i32.to_variant()));
        app.change_action_state("pub-wrapper-action13", &130i32.to_variant());
        app.activate_action("pub-wrapper-action14", None);
        app.activate_action("pub-wrapper-action15", Some(&"Hello".to_variant()));
        glib::MainContext::default().block_on(async move {
            glib::timeout_future_seconds(0).await;
        });
        let log = app.take_log();
        assert_eq!(
            log,
            [
                "pub-wrapper-action1",
                "pub-wrapper-action2",
                "pub-wrapper-action3 3",
                "pub-wrapper-action4",
                "pub-wrapper-action5 5",
                "pub-wrapper-action6 6",
                "pub-wrapper-action7 7",
                "pub-wrapper-action8 8",
                "pub-wrapper-action8 change 80",
                "pub-wrapper-action9",
                "pub-wrapper-action10 10",
                "pub-wrapper-action11 (11, 111)",
                "pub-wrapper-action12 12",
                "pub-wrapper-action12 change 12",
                "pub-wrapper-action12 change 120",
                "pub-wrapper-action13 13",
                "pub-wrapper-action13 change 13",
                "pub-wrapper-action13 change 130",
                "pub-wrapper-action14",
                "pub-wrapper-action15 Hello",
            ]
        );

        app.lookup_action("pub-wrapper-action6")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);

        app.pub_wrapper_action1();
        app.pub_wrapper_action2();
        app.pub_wrapper_action3(3);
        app.lookup_action("pub-wrapper-action3")
            .unwrap()
            .downcast::<gio::SimpleAction>()
            .unwrap()
            .set_enabled(true);
        app.pub_wrapper_action3(3);
        app.pub_wrapper_action4();
        app.pub_wrapper_action5(5);
        app.pub_wrapper_action6(6);
        app.pub_wrapper_action6(6);
        app.pub_wrapper_action7(7);
        app.pub_wrapper_action8(8);
        app.pub_wrapper_action8_change_state(80);
        app.pub_wrapper_action9();
        app.pub_wrapper_action10(10);
        app.pub_wrapper_action11((11i32, 111i32).to_variant());
        app.pub_wrapper_action12(12);
        app.pub_wrapper_action12_change_state(120);
        app.pub_wrapper_action13(13i32.to_variant());
        app.pub_wrapper_action13_change_state(130i32.to_variant());
        glib::MainContext::default().block_on(async move {
            app.pub_wrapper_action14().await;
            app.pub_wrapper_action15("Hello".into()).await;
        });
        let log = app.take_log();
        assert_eq!(
            log,
            [
                "pub-wrapper-action1",
                "pub-wrapper-action2",
                "pub-wrapper-action3 3",
                "pub-wrapper-action4",
                "pub-wrapper-action5 5",
                "pub-wrapper-action6 6",
                "pub-wrapper-action7 7",
                "pub-wrapper-action8 8",
                "pub-wrapper-action8 change 80",
                "pub-wrapper-action9",
                "pub-wrapper-action10 10",
                "pub-wrapper-action11 (11, 111)",
                "pub-wrapper-action12 12",
                "pub-wrapper-action12 change 12",
                "pub-wrapper-action12 change 120",
                "pub-wrapper-action13 13",
                "pub-wrapper-action13 change 13",
                "pub-wrapper-action13 change 130",
                "pub-wrapper-action14",
                "pub-wrapper-action15 Hello",
            ]
        );

        app.quit();
    });
    app.run_with_args::<&str>(&[]);
}

#[derive(Default, glib::Downgrade)]
pub struct GroupContainer {
    log: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

#[gobject::actions]
impl GroupContainer {
    fn log(&self, msg: &str) {
        self.log.borrow_mut().push(msg.to_owned());
    }
    fn take_log(&self) -> Vec<String> {
        self.log.take()
    }
    #[action(with = "gobject::variant::glib::uri")]
    fn my_action(&self, uri: glib::Uri) {
        self.log(&format!("action {}", uri));
    }
}

#[test]
fn action_group() {
    use gio::prelude::*;

    let container = GroupContainer::default();
    let group = gio::SimpleActionGroup::new();
    container.register_actions(&group);
    group.activate_action("my-action", Some(&"file:///hello".to_variant()));
    let log = container.take_log();
    assert_eq!(log, ["action file:///hello"]);
}
