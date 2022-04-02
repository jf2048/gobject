use gobject_core::{ClassDefinition, ClassOptions, TypeBase};
use quote::ToTokens;

#[test]
fn properties() {
    let module = syn::parse_quote! {
        mod basic {
            use glib::once_cell::unsync::OnceCell;
            use std::cell::{Cell, RefCell};
            use std::marker::PhantomData;
            use std::sync::{Mutex, RwLock};
            use glib::subclass::prelude::ObjectImplExt;

            #[properties]
            #[derive(Default)]
            pub struct BasicProps {
                #[property(get)]
                readable_i32: Cell<i32>,
                #[property(set)]
                writable_i32: Cell<i32>,
                #[property(get, set)]
                my_i32: Cell<i32>,
                #[property(get, set, borrow)]
                my_str: RefCell<String>,
                #[property(get, set)]
                my_mutex: Mutex<i32>,
                #[property(get, set)]
                my_rw_lock: RwLock<String>,
                #[property(
                    get,
                    set,
                    construct,
                    name = "my-u8",
                    nick = "My U8",
                    blurb = "A uint8",
                    builder(minimum = 5, maximum = 20, default_value = 19)
                )]
                my_attributed: Cell<u8>,
                #[property(get, set, construct_only, builder(default_value = 100.0))]
                my_construct_only: Cell<f64>,
                #[property(get, set, explicit_notify, lax_validation)]
                my_explicit: Cell<u64>,
                #[property(get, set, explicit_notify, lax_validation)]
                my_auto_set: OnceCell<f32>,
                #[property(get, set, explicit_notify, lax_validation, construct_only)]
                my_auto_set_co: OnceCell<f32>,
                #[property(get = "_", set = "_", explicit_notify, lax_validation)]
                my_custom_accessors: RefCell<String>,
                #[property(computed, get, set, explicit_notify)]
                my_computed_prop: PhantomData<i32>,
                #[property(get, set, storage = "inner.my_bool")]
                my_delegate: Cell<bool>,
                #[property(get, set, notify = false, connect_notify = false)]
                my_no_defaults: Cell<u64>,

                inner: BasicPropsInner,
            }

            #[derive(Default)]
            struct BasicPropsInner {
                my_bool: Cell<bool>,
            }

            #[methods]
            impl BasicProps {
                fn constructed(&self, obj: &Self::Type) {
                    self.parent_constructed(obj);
                    obj.connect_my_i32_notify(|obj| obj.notify_my_computed_prop());
                }
                pub fn my_custom_accessors(&self, obj: &super::BasicProps) -> String {
                    self.my_custom_accessors.borrow().clone()
                }
                pub fn set_my_custom_accessors(&self, obj: &super::BasicProps, value: String) {
                    let old = self.my_custom_accessors.replace(value);
                    if old != *self.my_custom_accessors.borrow() {
                        obj.notify_my_custom_accessors();
                    }
                }
                pub fn my_computed_prop(&self, obj: &super::BasicProps) -> i32 {
                    self.my_i32.get() + 7
                }
                fn set_my_computed_prop(&self, obj: &super::BasicProps, value: i32) {
                    obj.set_my_i32(value - 7);
                }
            }
        }
    };
    let mut errors = vec![];
    let attr = quote::quote! { final };
    let opts = ClassOptions::parse(attr, &mut errors);
    let parser = ClassDefinition::type_parser();
    let go = quote::format_ident!("go");
    let type_def = parser.parse(module, TypeBase::Class, go, &mut errors);
    let class_def = ClassDefinition::from_type(type_def, opts, &mut errors);
    let _tokens = class_def.to_token_stream();
    if !errors.is_empty() {
        panic!("{}", darling::Error::multiple(errors));
    }
}
