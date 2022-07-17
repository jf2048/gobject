use gobject_core::util::{self, Errors};
use proc_macro::TokenStream;
use quote::ToTokens;

#[cfg(any(feature = "gtk4", feature = "gio"))]
mod actions;
#[cfg(feature = "gtk4")]
mod gtk4_actions;
#[cfg(feature = "gtk4")]
mod gtk4_templates;
#[cfg(any(feature = "gtk4", feature = "gio"))]
mod initable;
#[cfg(feature = "serde")]
mod serde;
#[cfg(feature = "variant")]
mod variant;

#[proc_macro_attribute]
#[proc_macro_error::proc_macro_error]
pub fn clone_block(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut item = syn::parse_macro_input!(item as syn::Item);
    let errors = Errors::new();
    let go = crate_path();
    gobject_core::closures(&mut item, &go, &errors);
    append_errors(item.to_token_stream(), errors)
}

#[proc_macro_derive(Properties, attributes(properties, property))]
pub fn derive_properties(item: TokenStream) -> TokenStream {
    let errors = Errors::new();
    let tokens = util::parse::<syn::DeriveInput>(item.into(), &errors)
        .map(|input| {
            let go = crate_path();
            gobject_core::derived_class_properties(&input, &go, &errors)
        })
        .unwrap_or_default();
    append_errors(tokens, errors)
}

#[proc_macro_attribute]
pub fn class(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{ClassDefinition, ClassOptions};

    let errors = Errors::new();
    let opts = ClassOptions::parse(attr.into(), &errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &errors);
    let tokens = module
        .map(|module| {
            let go = crate_path();
            let mut class = ClassDefinition::parse(module, opts, go, &errors);
            let _parent_type: Option<syn::Path> = (!class.extends.is_empty()).then(|| {
                let ident = class.parent_type_alias();
                syn::parse_quote! { super::#ident }
            });
            #[cfg(any(feature = "gtk4", feature = "gio"))]
            actions::extend_actions(&mut class, &errors);
            #[cfg(any(feature = "gtk4", feature = "gio"))]
            initable::extend_initables(&mut class, &errors);
            #[cfg(feature = "variant")]
            variant::extend_variant(
                &mut class.inner,
                class.final_,
                class.abstract_,
                _parent_type.as_ref(),
                class.ext_trait.as_ref(),
                &errors,
            );
            #[cfg(feature = "serde")]
            serde::extend_serde(
                &mut class.inner,
                class.final_,
                class.abstract_,
                _parent_type.as_ref(),
                class.ext_trait.as_ref(),
                class.ns.as_ref(),
                &errors,
            );

            class.add_private_items();
            class.to_token_stream()
        })
        .unwrap_or_default();
    append_errors(tokens, errors)
}

#[proc_macro_attribute]
pub fn interface(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{InterfaceDefinition, InterfaceOptions};

    let errors = Errors::new();
    let opts = InterfaceOptions::parse(attr.into(), &errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &errors);
    let tokens = module
        .map(|module| {
            let go = crate_path();
            let mut iface = InterfaceDefinition::parse(module, opts, go, &errors);
            #[cfg(feature = "variant")]
            variant::extend_variant(
                &mut iface.inner,
                false,
                true,
                None,
                Some(&iface.ext_trait),
                &errors,
            );
            #[cfg(feature = "serde")]
            serde::extend_serde(
                &mut iface.inner,
                false,
                true,
                None,
                Some(&iface.ext_trait),
                iface.ns.as_ref(),
                &errors,
            );
            iface.add_private_items(&errors);
            iface.to_token_stream()
        })
        .unwrap_or_default();
    append_errors(tokens, errors)
}

#[cfg(feature = "variant")]
#[proc_macro]
pub fn variant_cast(input: TokenStream) -> TokenStream {
    let errors = Errors::new();
    let go = crate_path();
    let output = variant::downcast_enum(input.into(), &go, &errors);
    append_errors(output, errors)
}

#[cfg(feature = "serde")]
#[proc_macro]
pub fn serde_cast(input: TokenStream) -> TokenStream {
    let errors = Errors::new();
    let go = crate_path();
    let output = serde::downcast_enum(input.into(), &go, &errors);
    append_errors(output, errors)
}

#[cfg(any(feature = "gtk4", feature = "gio"))]
#[proc_macro_attribute]
pub fn group_actions(attr: TokenStream, item: TokenStream) -> TokenStream {
    let errors = Errors::new();
    let tokens = util::parse::<syn::ItemImpl>(item.into(), &errors)
        .map(|impl_| {
            let go = crate_path();
            actions::impl_group_actions(impl_, attr.into(), &go, &errors)
        })
        .unwrap_or_default();
    append_errors(tokens, errors)
}

#[cfg(feature = "gtk4")]
#[proc_macro_attribute]
pub fn gtk4_widget(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{ClassDefinition, ClassOptions};

    let errors = Errors::new();
    let opts = ClassOptions::parse(attr.into(), &errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &errors);
    let tokens = module
        .map(|module| {
            let go = crate_path();
            let mut class = ClassDefinition::parse(module, opts, go.clone(), &errors);
            class.extends.push(syn::parse_quote! { #go::gtk4::Widget });
            class
                .inherits
                .push(syn::parse_quote! { #go::gtk4::Accessible });
            class
                .inherits
                .push(syn::parse_quote! { #go::gtk4::Buildable });
            class
                .inherits
                .push(syn::parse_quote! { #go::gtk4::ConstraintTarget });
            if class.parent_trait.is_none() {
                class.parent_trait = Some(syn::parse_quote! {
                    #go::gtk4::subclass::prelude::WidgetImpl
                });
            }
            actions::extend_actions(&mut class, &errors);
            initable::extend_initables(&mut class, &errors);
            gtk4_templates::extend_template(&mut class, &errors);
            gtk4_actions::extend_widget_actions(&mut class, &errors);
            class.add_private_items();
            class.to_token_stream()
        })
        .unwrap_or_default();
    append_errors(tokens, errors)
}

#[cfg(feature = "use_gst")]
#[proc_macro_attribute]
pub fn gst_element(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::gst;

    let errors = Errors::new();
    let opts = gst::ElementOptions::parse(attr.into(), &errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &errors);
    let tokens = module
        .map(|module| {
            let go = crate_path();
            let element = gst::ElementDefinition::parse(module, opts, go, &errors);

            element.to_token_stream()
        })
        .unwrap_or_default();
    append_errors(tokens, errors)
}

#[inline]
fn append_errors(mut tokens: proc_macro2::TokenStream, errors: Errors) -> TokenStream {
    if let Some(errors) = errors.into_compile_errors() {
        tokens.extend(errors);
    }
    tokens.into()
}

#[inline]
fn crate_path() -> syn::Path {
    use proc_macro_crate::FoundCrate;

    let crate_name = match proc_macro_crate::crate_name("gobject") {
        Ok(FoundCrate::Name(name)) => name,
        Ok(FoundCrate::Itself) => "gobject".into(),
        Err(e) => {
            proc_macro_error::emit_error!("{}", e);
            "gobject".into()
        }
    };

    let ident = syn::Ident::new(&crate_name, proc_macro2::Span::call_site());
    syn::parse_quote! { #ident }
}
