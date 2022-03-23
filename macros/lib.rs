use proc_macro::TokenStream;

mod class_impl;
mod interface_impl;
mod property;
mod signal;
mod type_definition;
mod util;
mod validations;
mod virtual_method;

#[proc_macro_attribute]
pub fn class(attr: TokenStream, item: TokenStream) -> TokenStream {
    let mut errors = vec![];
    let opts = util::parse_list::<class_impl::Options>(attr.into(), &mut errors);
    let module = util::parse::<syn::ItemMod>(item.into(), &mut errors);
    let tokens = module
        .map(|module| class_impl::class_impl(opts, module, &mut errors))
        .unwrap_or_default();
    if !errors.is_empty() {
        darling::Error::multiple(errors).write_errors().into()
    } else {
        tokens.into()
    }
}
