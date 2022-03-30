use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn class(attr: TokenStream, item: TokenStream) -> TokenStream {
    use gobject_core::{ClassDefinition, ClassOptions, util};

    let mut errors = vec![];
    let opts = ClassOptions::parse(attr.into(), &mut errors);
    let parser = ClassDefinition::type_parser();
    let module = util::parse::<syn::ItemMod>(item.into(), &mut errors);
    let tokens = module.map(|module| {
        let type_def = parser.parse(module, false, &mut errors);
        let go = crate_ident();
        let class_def = ClassDefinition::from_type(type_def, opts, go.clone(), &mut errors);
        class_def.to_tokens()
    }).unwrap_or_default();
    if errors.is_empty() {
        tokens.into()
    } else {
        darling::Error::multiple(errors).write_errors().into()
    }
}

#[proc_macro_attribute]
pub fn interface(attr: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_attribute]
pub fn gtk_widget(attr: TokenStream, item: TokenStream) -> TokenStream {
    item
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
