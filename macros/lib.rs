use gobject_core::util::{self, Errors};
use proc_macro::TokenStream;
use quote::ToTokens;

#[cfg(feature = "gtk4")]
mod gtk4;
#[cfg(feature = "serde")]
mod serde;

#[proc_macro_attribute]
#[proc_macro_error::proc_macro_error]
pub fn clone_block(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut item = syn::parse_macro_input!(item as syn::Item);
    let errors = Errors::new();
    let go = crate_ident();
    gobject_core::closures(&mut item, go, &errors);
    append_errors(item.to_token_stream(), errors)
}

#[proc_macro_derive(Properties, attributes(properties, property))]
pub fn derive_properties(item: TokenStream) -> TokenStream {
    let errors = Errors::new();
    let tokens = util::parse::<syn::DeriveInput>(item.into(), &errors)
        .map(|input| {
            let go = crate_ident();
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
            let go = crate_ident();
            let mut _class_def = ClassDefinition::parse(module, opts, go, &errors);
            #[cfg(feature = "serde")]
            {
                let parent_type = (!_class_def.extends.is_empty())
                    .then(|| {
                        let ident = _class_def.parent_type_alias()?;
                        Some(syn::parse_quote! { super::#ident })
                    })
                    .flatten();
                serde::extend_serde(
                    &mut _class_def.inner,
                    _class_def.final_,
                    _class_def.abstract_,
                    parent_type.as_ref(),
                    _class_def.ext_trait.as_ref(),
                    _class_def.ns.as_ref(),
                    &errors,
                );
            }
            _class_def.to_token_stream()
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
            let go = crate_ident();
            let mut _iface_def = InterfaceDefinition::parse(module, opts, go, &errors);
            #[cfg(feature = "serde")]
            {
                serde::extend_serde(
                    &mut _iface_def.inner,
                    false,
                    true,
                    None,
                    _iface_def.ext_trait.as_ref(),
                    _iface_def.ns.as_ref(),
                    &errors,
                );
            }
            _iface_def.to_token_stream()
        })
        .unwrap_or_default();
    append_errors(tokens, errors)
}

#[cfg(feature = "serde")]
#[proc_macro]
pub fn serde_cast(input: TokenStream) -> TokenStream {
    let errors = Errors::new();
    let go = crate_ident();
    let output = serde::downcast_enum(input.into(), &go, &errors);
    append_errors(output, errors)
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
            let go = crate_ident();
            let mut class_def = ClassDefinition::parse(module, opts, go, &errors);
            gtk4::extend_gtk4(&mut class_def, &errors);
            class_def.to_token_stream()
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
