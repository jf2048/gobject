use gobject_core::{ClassDefinition, ClassOptions, TypeBase};
use quote::ToTokens;

#[test]
fn properties() {
    let module = syn::parse_quote! {
        mod obj_abstract {
            use glib::subclass::types::ObjectSubclassExt;

            #[derive(Default)]
            pub struct ObjDerivable {
                #[property(get, set, abstract)]
                my_prop: std::marker::PhantomData<u64>,
            }
            impl ObjDerivable {
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
    };
    let mut errors = vec![];
    let attr = quote::quote! { abstract };
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

