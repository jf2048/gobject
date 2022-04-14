use gobject_core::util;
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
            let mut _class_def = ClassDefinition::parse(module, opts, go, &mut errors);
            #[cfg(feature = "serde")]
            {
                let parent_type = (!_class_def.extends.is_empty())
                    .then(|| {
                        let name = _class_def.inner.name.as_ref()?;
                        let ident = quote::format_ident!("{}ParentType", name);
                        Some(syn::parse_quote! { super::#ident })
                    })
                    .flatten();
                let ext_trait = _class_def.ext_trait();
                serde::extend_serde(
                    &mut _class_def.inner,
                    _class_def.final_,
                    _class_def.abstract_,
                    parent_type.as_ref(),
                    ext_trait.as_ref(),
                    _class_def.ns.as_ref(),
                    &mut errors,
                );
            }
            _class_def.to_token_stream()
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
            let mut _iface_def = InterfaceDefinition::parse(module, opts, go, &mut errors);
            #[cfg(feature = "serde")]
            {
                let ext_trait = _iface_def.ext_trait();
                serde::extend_serde(
                    &mut _iface_def.inner,
                    false,
                    true,
                    None,
                    ext_trait.as_ref(),
                    _iface_def.ns.as_ref(),
                    &mut errors,
                );
            }
            _iface_def.to_token_stream()
        })
        .unwrap_or_default();
    tokens_or_error(tokens, errors)
}

#[cfg(feature = "serde")]
#[proc_macro]
pub fn serde_cast(input: TokenStream) -> TokenStream {
    let mut errors = vec![];
    let go = crate_ident();
    let output = serde::downcast_enum(input.into(), &go, &mut errors);
    tokens_or_error(output, errors)
}

#[cfg(feature = "gtk4")]
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
            gtk4::extend_gtk4(&mut class_def);
            class_def.to_token_stream()
        })
        .unwrap_or_default();
    tokens_or_error(tokens, errors)
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
