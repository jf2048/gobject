use gobject_core::util;
use proc_macro::TokenStream;
use quote::ToTokens;

#[proc_macro_derive(Properties, attributes(properties, property))]
pub fn derive_properties(item: TokenStream) -> TokenStream {
    let mut errors = vec![];
    let tokens = util::parse::<syn::DeriveInput>(item.into(), &mut errors).map(|input| {
        let go = crate_ident();
        gobject_core::derived_class_properties(&input, &go, &mut errors)
    }).unwrap_or_default();
    tokens_or_error(tokens, errors)
}

#[proc_macro_attribute]
pub fn class(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{ClassDefinition, ClassOptions, TypeBase};

    let mut errors = vec![];
    let opts = ClassOptions::parse(attr.into(), &mut errors);
    let parser = ClassDefinition::type_parser();
    let module = util::parse::<syn::ItemMod>(item.into(), &mut errors);
    let tokens = module.map(|module| {
        let type_def = parser.parse(module, TypeBase::Class, &mut errors);
        let go = crate_ident();
        let class_def = ClassDefinition::from_type(type_def, opts, go.clone(), &mut errors);
        class_def.to_token_stream()
    }).unwrap_or_default();
    tokens_or_error(tokens, errors)
}

#[proc_macro_attribute]
pub fn interface(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{InterfaceDefinition, InterfaceOptions, TypeBase};

    let mut errors = vec![];
    let opts = InterfaceOptions::parse(attr.into(), &mut errors);
    let parser = InterfaceDefinition::type_parser();
    let module = util::parse::<syn::ItemMod>(item.into(), &mut errors);
    let tokens = module.map(|module| {
        let type_def = parser.parse(module, TypeBase::Interface, &mut errors);
        let go = crate_ident();
        let class_def = InterfaceDefinition::from_type(type_def, opts, go.clone(), &mut errors);
        class_def.to_token_stream()
    }).unwrap_or_default();
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
        },
    };

    syn::Ident::new(&crate_name, proc_macro2::Span::call_site())
}
