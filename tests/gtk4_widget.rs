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
        #[action]
        my_string: std::cell::RefCell<String>,
        #[template_child]
        label: gtk4::TemplateChild<gtk4::Label>,
        #[template_child(id = "my_label2")]
        #[property(get, object)]
        label2: gtk4::TemplateChild<gtk4::Label>,
        #[template_child]
        button: gtk4::TemplateChild<gtk4::Button>,
        #[action_group]
        pad: gobject::WeakCell<gtk4::gio::SimpleActionGroup>,
    }
    impl MyWidget {
        fn constructed(&self, obj: &super::MyWidget) {
            self.parent_constructed(obj);
            let pad = gtk4::PadController::new(&self.pad.upgrade().unwrap(), None);
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
        #[action]
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
    #[widget_actions(group = "pad")]
    impl MyWidget {
        #[action]
        fn pad_button(&self) {
            self.label.set_label("Pad button");
        }
    }
    impl super::MyWidget {
        #[constructor(infallible)]
        pub fn new(my_string: &str) -> Self {}
        #[action(name = "static-action")]
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
