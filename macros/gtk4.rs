use gobject_core::ClassDefinition;
use proc_macro2::Span;
use quote::ToTokens;
use syn::{parse_quote, parse_quote_spanned};

pub(crate) fn extend_gtk4(def: &mut ClassDefinition) {
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
    let has_callbacks = if let Some(impl_) = def.inner.methods_item_mut() {
        if impl_.items.iter().any(|a| match a {
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
            }) {
                impl_
                    .attrs
                    .push(parse_quote! { #[#gtk4::template_callbacks] });
            }
            true
        } else {
            false
        }
    } else {
        false
    };
    if has_callbacks {
        def.inner.add_custom_stmt("class_init", parse_quote_spanned! { Span::mixed_site() =>
            #gtk4::subclass::widget::CompositeTemplateCallbacksClass::bind_template_callbacks(____class);
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
