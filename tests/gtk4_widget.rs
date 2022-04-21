#![cfg(feature = "gtk4_macros")]

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
        label2: gtk4::TemplateChild<gtk4::Label>,
    }
    impl MyWidget {
        #[template_callback]
        fn on_clicked(&self, button: &gtk4::Button) {
            button.set_label("Clicked");
        }
        fn dispose(&self, obj: &super::MyWidget) {
            while let Some(child) = obj.first_child() {
                child.unparent();
            }
        }
    }
    impl WidgetImpl for MyWidget {}
}
