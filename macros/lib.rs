use gobject_core::util;
use proc_macro::TokenStream;
use quote::ToTokens;

#[proc_macro_attribute]
#[proc_macro_error::proc_macro_error]
pub fn clone_block(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut item = syn::parse_macro_input!(item as syn::Item);
    let mut errors = vec![];
    let go = crate_ident();
    gobject_core::closures(&mut item, go, &mut errors);
    for error in errors {
        proc_macro_error::Diagnostic::from(error).emit();
    }
    item.to_token_stream().into()
}

#[proc_macro_derive(Properties, attributes(properties, property))]
pub fn derive_properties(item: TokenStream) -> TokenStream {
    let mut errors = vec![];
    let tokens = util::parse::<syn::DeriveInput>(item.into(), &mut errors)
        .map(|input| {
            let go = crate_ident();
            gobject_core::derived_class_properties(&input, &go, &mut errors)
        })
        .unwrap_or_default();
    tokens_or_error(tokens, errors)
}

#[proc_macro_attribute]
pub fn class(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{ClassDefinition, ClassOptions};

    let mut errors = vec![];
    let opts = ClassOptions::parse(attr.into(), &mut errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &mut errors);
    let tokens = module
        .map(|module| {
            let go = crate_ident();
            let class_def = ClassDefinition::parse(module, opts, go, &mut errors);
            class_def.to_token_stream()
        })
        .unwrap_or_default();
    tokens_or_error(tokens, errors)
}

#[proc_macro_attribute]
pub fn interface(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{InterfaceDefinition, InterfaceOptions};

    let mut errors = vec![];
    let opts = InterfaceOptions::parse(attr.into(), &mut errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &mut errors);
    let tokens = module
        .map(|module| {
            let go = crate_ident();
            let class_def = InterfaceDefinition::parse(module, opts, go, &mut errors);
            class_def.to_token_stream()
        })
        .unwrap_or_default();
    tokens_or_error(tokens, errors)
}

#[proc_macro_attribute]
pub fn gtk4_widget(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{ClassDefinition, ClassOptions};

    let mut errors = vec![];
    let opts = ClassOptions::parse(attr.into(), &mut errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &mut errors);
    let tokens = module
        .map(|module| {
            let go = crate_ident();
            let mut class_def = ClassDefinition::parse(module, opts, go, &mut errors);
            extend_gtk4(&mut class_def);
            class_def.to_token_stream()
        })
        .unwrap_or_default();
    tokens_or_error(tokens, errors)
}

fn extend_gtk4(def: &mut gobject_core::ClassDefinition) {
    use proc_macro2::Span;
    use syn::{parse_quote, parse_quote_spanned};

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

#[allow(dead_code)]
#[inline]
fn path_to_string(path: &syn::Path) -> String {
    path.to_token_stream()
        .into_iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("")
}

#[inline]
fn tokens_or_error(tokens: proc_macro2::TokenStream, errors: Vec<darling::Error>) -> TokenStream {
    if errors.is_empty() {
        tokens.into()
    } else {
        darling::Error::multiple(errors).write_errors().into()
    }
}

#[inline]
fn crate_ident() -> syn::Ident {
    use proc_macro_crate::FoundCrate;

    let crate_name = match proc_macro_crate::crate_name("gobject") {
        Ok(FoundCrate::Name(name)) => name,
        Ok(FoundCrate::Itself) => "gobject".into(),
        Err(e) => {
            proc_macro_error::emit_error!("{}", e);
            "gobject".into()
        }
    };

    syn::Ident::new(&crate_name, proc_macro2::Span::call_site())
}
