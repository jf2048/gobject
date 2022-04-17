use gobject_core::{util::Errors, ClassDefinition, TypeMode};
use proc_macro2::Span;
use quote::ToTokens;
use std::collections::HashSet;
use syn::{parse_quote, parse_quote_spanned};

pub(crate) fn extend_gtk4(def: &mut ClassDefinition, errors: &Errors) {
    let go = &def.inner.crate_ident;
    let gtk4 = quote::quote! { #go::gtk4 };
    if let Some(struct_) = def.inner.properties_item() {
        if struct_.attrs.iter().any(|a| a.path.is_ident("template")) {
            def.inner.add_custom_stmt(
                "class_init",
                parse_quote_spanned! { Span::mixed_site() =>
                    #gtk4::subclass::widget::CompositeTemplateClass::bind_template(____class);
                },
            );
            def.inner.add_custom_stmt(
                "instance_init",
                parse_quote_spanned! { Span::mixed_site() =>
                    #gtk4::prelude::InitializingWidgetExt::init_template(obj);
                },
            );
        }
    }
    let mut added_callbacks = HashSet::new();
    let mut has_callbacks = false;
    let mut has_instance_callbacks = false;
    for (index, impl_) in def.inner.methods_items_mut().enumerate() {
        if let Some(callback) = impl_.items.iter().find(|a| match a {
            syn::ImplItem::Method(m) => {
                m.attrs.iter().any(|a| a.path.is_ident("template_callback"))
            }
            _ => false,
        }) {
            if !impl_.attrs.iter().any(|a| {
                let s = path_to_string(&a.path);
                s == "template_callbacks"
                    || s == "gtk::template_callbacks"
                    || s == "gtk4::template_callbacks"
                    || s == "::gtk::template_callbacks"
                    || s == "::gtk4::template_callbacks"
                    || s == "gobject::gtk4::template_callbacks"
                    || s == "::gobject::gtk4::template_callbacks"
            }) {
                added_callbacks.insert(index);
                impl_
                    .attrs
                    .push(parse_quote! { #[#gtk4::template_callbacks] });
            }
            match TypeMode::for_item_type(&*impl_.self_ty) {
                Some(TypeMode::Subclass) => {
                    if has_callbacks {
                        errors.push_spanned(
                            callback,
                            "Only one impl block can contain subclass callbacks",
                        );
                    }
                    has_callbacks = true;
                }
                Some(TypeMode::Wrapper) => {
                    if has_instance_callbacks {
                        errors.push_spanned(
                            callback,
                            "Only one impl block can contain instance callbacks",
                        );
                    }
                    has_instance_callbacks = true;
                }
                _ => {}
            }
        }
    }
    if !added_callbacks.is_empty() {
        for (index, impl_) in def.inner.methods_items_mut().enumerate() {
            if added_callbacks.contains(&index) {
                *impl_ = parse_quote! {
                    const _: () = {
                        use #gtk4;
                        #impl_
                    };
                };
            }
        }
    }
    if has_callbacks {
        def.inner.add_custom_stmt("class_init", parse_quote_spanned! { Span::mixed_site() =>
            #gtk4::subclass::widget::CompositeTemplateCallbacksClass::bind_template_callbacks(____class);
        });
    }
    if has_instance_callbacks {
        def.inner.add_custom_stmt("class_init", parse_quote_spanned! { Span::mixed_site() =>
            #gtk4::subclass::widget::CompositeTemplateInstanceCallbacksClass::bind_template_instance_callbacks(____class);
        });
    }
}

#[inline]
fn path_to_string(path: &syn::Path) -> String {
    path.to_token_stream()
        .into_iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("")
}
