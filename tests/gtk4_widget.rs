#![cfg(feature = "use_gtk4")]

static MY_FLAG: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

#[gobject::gtk4_widget(final)]
mod my_widget {
    use gtk4::prelude::*;
    use gtk4::subclass::prelude::*;

    #[derive(Debug, Default)]
    #[template(string = r#"
    <interface>
      <template class="MyWidget" parent="GtkWidget">
        <child>
          <object class="GtkLabel" id="label">
            <property name="label">foobar</property>
          </object>
        </child>
        <child>
          <object class="GtkLabel" id="my_label2">
            <property name="label">foobaz</property>
          </object>
        </child>
        <child>
          <object class="GtkButton" id="button">
            <property name="label">Button</property>
            <signal name="clicked" handler="on_clicked" swapped="true"/>
          </object>
        </child>
      </template>
    </interface>
    "#)]
    pub struct MyWidget {
        #[property(get, set)]
        #[widget_action]
        my_string: std::cell::RefCell<String>,
        #[template_child]
        label: gtk4::TemplateChild<gtk4::Label>,
        #[template_child(id = "my_label2")]
        #[property(get, object)]
        label2: gtk4::TemplateChild<gtk4::Label>,
        #[template_child]
        button: gtk4::TemplateChild<gtk4::Button>,
    }
    impl MyWidget {
        fn constructed(&self, obj: &super::MyWidget) {
            self.parent_constructed(obj);
            let pad_group = gio::SimpleActionGroup::new();
            obj.register_pad_actions(&pad_group);
            obj.insert_action_group("pad", Some(&pad_group));
            let pad = gtk4::PadController::new(&pad_group, None);
            pad.set_action(
                gtk4::PadActionType::Button,
                0,
                -1,
                "Set Label",
                "pad-button",
            );
            obj.add_controller(&pad);
        }
        #[template_callback]
        fn on_clicked(&self, button: &gtk4::Button) {
            button.set_label("Clicked");
        }
        #[public]
        fn click_and_get_button_label(&self) -> Option<glib::GString> {
            self.button.emit_clicked();
            self.button.label()
        }
        #[widget_action]
        fn set_label(&self, value: String) {
            self.label.set_label(&value);
        }
        #[public]
        fn label(&self) -> glib::GString {
            self.label.label()
        }
        fn dispose(&self, obj: &super::MyWidget) {
            while let Some(child) = obj.first_child() {
                child.unparent();
            }
        }
    }
    #[gobject::group_actions(register = "register_pad_actions")]
    impl super::MyWidget {
        #[group_action]
        fn pad_button(&self) {
            self.imp().label.set_label("Pad button");
        }
    }
    impl super::MyWidget {
        #[constructor(infallible)]
        pub fn new(my_string: &str) -> Self {}
        #[widget_action(name = "static-action")]
        #[public(name = "static_action")]
        fn _static_action() {
            super::MY_FLAG.store(true, std::sync::atomic::Ordering::Release);
        }
    }
    impl WidgetImpl for MyWidget {}
}

#[gtk4::test]
fn widget() {
    use gtk4::prelude::*;

    let widget = MyWidget::new("hello");
    assert_eq!(widget.my_string(), "hello");
    assert_eq!(widget.click_and_get_button_label().unwrap(), "Clicked");

    widget.action_set_enabled("my-widget.my-string", false);
    widget
        .activate_action("my-widget.my-string", Some(&"world".to_variant()))
        .unwrap();
    assert_eq!(widget.my_string(), "hello");

    widget.action_set_enabled("my-widget.my-string", true);
    widget
        .activate_action("my-widget.my-string", Some(&"world".to_variant()))
        .unwrap();
    assert_eq!(widget.my_string(), "world");

    widget
        .activate_action("my-widget.set-label", Some(&"New label".to_variant()))
        .unwrap();
    assert_eq!(widget.label(), "New label");
    widget.activate_action("pad.pad-button", None).unwrap();
    assert_eq!(widget.label(), "Pad button");
    widget.static_action();
    assert!(MY_FLAG.load(std::sync::atomic::Ordering::Acquire));
}

#[gobject::gtk4_widget(final)]
mod action_widget {
    #[derive(Default)]
    pub struct ActionWidget {}
    impl ActionWidget {
        #[widget_action]
        #[public]
        fn action1() {}
        #[widget_action(group = "stuff", name = "renamed-action2")]
        #[public]
        fn action2(&self) {}
        #[widget_action(disabled)]
        #[public]
        fn action3(_param: i32) {}
        #[widget_action]
        #[public]
        fn action4(&self, _param: i32) {}
        #[widget_action]
        #[public]
        async fn action5(&self, _param: i32) {}
        #[widget_action(type_str = "i")]
        #[public]
        fn action6(&self, _param: &glib::Variant) {}
    }
    impl gtk4::subclass::prelude::WidgetImpl for ActionWidget {}
}
