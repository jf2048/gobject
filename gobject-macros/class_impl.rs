use crate::{type_definition::TypeDefinition, util};
use darling::{
    util::{Flag, PathList},
    FromMeta,
};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;

#[derive(Debug, Default, FromMeta)]
pub struct Options {
    pub name: Option<syn::Ident>,
    pub ns: Option<syn::Ident>,
    pub ext_trait: Option<syn::Ident>,
    pub pod: Flag,
    pub wrapper: Option<bool>,
    #[darling(rename = "final")]
    pub final_: Flag,
    #[darling(default)]
    pub extends: PathList,
    #[darling(default)]
    pub implements: PathList,
}

impl Options {
    fn ensure_name(&mut self, def: &TypeDefinition, errors: &mut Vec<darling::Error>) {
        if self.name.is_none() {
            self.name = def.name.clone();
        }
        if self.name.is_none() {
            errors.push(
                syn::Error::new(
                    def.span.clone(),
                    "Class must have a `name = \"...\"` parameter or a #[properties] struct",
                )
                .into(),
            );
        }
    }
    #[inline]
    fn wrapper(&self, mod_ident: &syn::Ident, glib: &TokenStream) -> Option<TokenStream> {
        if !self.wrapper.unwrap_or(true) {
            return None;
        }
        let mut inherits = Vec::new();
        if !self.extends.is_empty() {
            inherits.push(quote! { @extends });
            for extend in &*self.extends {
                inherits.push(extend.to_token_stream());
            }
        }
        if !self.implements.is_empty() {
            inherits.push(quote! { @implements });
            for implement in &*self.implements {
                inherits.push(implement.to_token_stream());
            }
        }
        let name = self.name.as_ref()?;
        Some(quote! {
            #glib::wrapper! {
                pub struct #name(ObjectSubclass<#mod_ident::#name>) #(#inherits),*;
            }
        })
    }
    #[inline]
    fn ext_trait(&self) -> Option<syn::Ident> {
        if self.final_.into() {
            return None;
        }
        let name = self.name.as_ref()?;
        Some(self.ext_trait
            .clone()
            .unwrap_or_else(|| format_ident!("{}Ext", name)))
    }
}

pub(crate) fn class_impl(
    mut opts: Options,
    mut module: syn::ItemMod,
    errors: &mut Vec<darling::Error>,
) -> TokenStream {
    let def = TypeDefinition::new(&mut module, false, errors);
    opts.ensure_name(&def, errors);
    let go = util::crate_ident();
    let glib = quote! { #go::glib };
    let wrapper = opts.wrapper(&module.ident, &glib);
    let public_impls = opts.name.as_ref().map(|name| {
        let trait_name = opts.ext_trait();
        def.public_impls(name, trait_name.as_ref(), &glib)
    });
    quote_spanned! { module.span() =>
        #module
        #wrapper
        #public_impls
    }
}
