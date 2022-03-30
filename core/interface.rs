use crate::{util, TypeDefinition, TypeDefinitionParser};
use darling::{util::PathList, FromMeta};
use heck::ToUpperCamelCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct Attrs {
    pub name: Option<syn::Ident>,
    pub ns: Option<syn::Ident>,
    pub ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::Path>,
    pub wrapper: Option<bool>,
    pub requires: PathList,
}

#[derive(Debug)]
pub struct InterfaceOptions(Attrs);

impl InterfaceOptions {
    pub fn parse(tokens: TokenStream, errors: &mut Vec<darling::Error>) -> Self {
        Self(util::parse_list(tokens, errors))
    }
}

#[derive(Debug)]
pub struct InterfaceDefinition {
    pub inner: TypeDefinition,
    pub ns: Option<syn::Ident>,
    pub ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::Path>,
    pub wrapper: bool,
    pub requires: Vec<syn::Path>,
    pub extra_interface_init_stmts: Vec<TokenStream>,
}

impl InterfaceDefinition {
    pub fn type_parser() -> TypeDefinitionParser {
        let mut parser = TypeDefinitionParser::new();
        parser
            .add_custom_method("properties")
            .add_custom_method("signals")
            .add_custom_method("interface_init")
            .add_custom_method("type_init");
        parser
    }
    pub fn from_type(
        def: TypeDefinition,
        opts: InterfaceOptions,
        crate_ident: syn::Ident,
        errors: &mut Vec<darling::Error>,
    ) -> Self {
        let attrs = opts.0;

        let mut iface = Self {
            inner: def,
            ns: attrs.ns,
            ext_trait: attrs.ext_trait,
            parent_trait: attrs.parent_trait,
            wrapper: attrs.wrapper.unwrap_or(true),
            requires: (*attrs.requires).clone(),
            extra_interface_init_stmts: Vec::new(),
        };

        if let Some(name) = attrs.name {
            iface.inner.set_name(name);
        }
        if iface.inner.name.is_none() {
            util::push_error(
                errors,
                iface.inner.span(),
                "Interface must have a `name = \"...\"` parameter or a #[properties] struct",
            );
        }
        iface.inner.set_crate_ident(crate_ident);

        let extra = iface.extra_private_items();

        let (_, items) = iface
            .inner
            .module
            .content
            .get_or_insert_with(Default::default);
        items.extend(extra.into_iter());

        iface
    }
    #[inline]
    fn derived_method<F>(&self, method: &str, func: F) -> Option<TokenStream>
    where
        F: FnOnce(&str) -> Option<TokenStream>,
    {
        self.inner
            .has_custom_method(method)
            .then(|| func(format!("derived_{}", method).as_str()))
            .flatten()
    }
    fn extra_private_items(&self) -> Vec<syn::Item> {
        let derived_methods = [
            self.derived_method("properties", |n| self.inner.properties_method(n)),
            self.derived_method("signals", |_| self.inner.derived_signals_method()),
            self.derived_method("interface_init", |n| self.interface_init_method(n)),
        ]
        .into_iter()
        .filter_map(|t| t)
        .collect::<Vec<_>>();
        let derived_methods = (!derived_methods.is_empty())
            .then(|| self.inner.name.as_ref())
            .flatten()
            .map(|name| {
                let head = if let Some(generics) = &self.inner.generics {
                    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
                    quote! { impl #impl_generics #name #type_generics #where_clause }
                } else {
                    quote! { impl #name }
                };
                quote! {
                    #head {
                        #(pub(super) #derived_methods)*
                    }
                }
            });
        self.inner
            .extra_private_items()
            .into_iter()
            .chain(
                [
                    self.object_interface_impl(),
                    self.interface_struct_definition(),
                    derived_methods,
                ]
                .into_iter()
                .filter_map(|t| t),
            )
            .map(syn::Item::Verbatim)
            .collect()
    }
    #[inline]
    fn wrapper(&self) -> Option<TokenStream> {
        if !self.wrapper {
            return None;
        }
        let mut requires = Vec::new();
        if !self.requires.is_empty() {
            requires.push(quote! { @requires });
            for require in &*self.requires {
                requires.push(require.to_token_stream());
            }
        }
        let mod_name = &self.inner.module.ident;
        let name = self.inner.name.as_ref()?;
        let glib = self.inner.glib()?;
        let generics = self.inner.generics.as_ref();
        Some(quote! {
            #glib::wrapper! {
                pub struct #name #generics(ObjectInterface<self::#mod_name::#name #generics>) #(#requires),*;
            }
        })
    }
    #[inline]
    fn ext_trait(&self) -> Option<syn::Ident> {
        let name = self.inner.name.as_ref()?;
        Some(
            self.ext_trait
                .clone()
                .unwrap_or_else(|| format_ident!("{}Ext", name)),
        )
    }
    fn interface_init_method(&self, method_name: &str) -> Option<TokenStream> {
        let iface_ident = syn::Ident::new("self", Span::mixed_site());
        let method_name = format_ident!("{}", method_name);
        let body = self.inner.type_init_body(&iface_ident);
        let extra = &self.extra_interface_init_stmts;
        if body.is_none() && extra.is_empty() {
            return None;
        }
        Some(quote! {
            fn #method_name(&mut self) {
                #body
                #(#extra)*
            }
        })
    }
    fn interface_struct_definition(&self) -> Option<TokenStream> {
        let fields = self.inner.type_struct_fields();
        let name = self.inner.name.as_ref()?;
        let generics = self.inner.generics.as_ref()?;
        let iface_name = format_ident!("{}Interface", name);
        let glib = self.inner.glib()?;
        Some(quote! {
            #[repr(C)]
            pub struct #iface_name #generics {
                pub parent_iface: #glib::gobject_ffi::GTypeInterface,
                #(pub #fields),*
            }
        })
    }
    #[inline]
    fn object_interface_impl(&self) -> Option<TokenStream> {
        let glib = self.inner.glib()?;
        let name = self.inner.name.as_ref()?;
        let head = if let Some(generics) = &self.inner.generics {
            let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
            quote! {
                impl #impl_generics #glib::subclass::types::ObjectInterface
                for #name #type_generics #where_clause
            }
        } else {
            quote! { impl #glib::subclass::types::ObjectInterface for #name }
        };
        let gtype_name = if let Some(ns) = &self.ns {
            format!("{}{}", ns, name)
        } else {
            name.to_string()
        }
        .to_upper_camel_case();
        let prerequisites = &self.requires;
        let interface_init = self
            .inner
            .custom_method("interface_init")
            .or_else(|| self.interface_init_method("interface_init"));
        let properties = self
            .inner
            .custom_method("properties")
            .or_else(|| self.inner.properties_method("properties"));
        let signals = self
            .inner
            .custom_method("signals")
            .or_else(|| self.inner.signals_method());
        let extra = self.inner.custom_methods(&["type_init"]);
        Some(quote! {
            #[#glib::object_interface]
            #head {
                const NAME: &'static ::std::primitive::str = #gtype_name;
                type Prerequisites = (#(#prerequisites,)*);
                #extra
                #interface_init
                #properties
                #signals
            }
        })
    }
    #[inline]
    fn is_implementable_impl(&self) -> Option<TokenStream> {
        let glib = self.inner.glib()?;
        let name = self.inner.name.as_ref()?;
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());
        let iface_ident = syn::Ident::new("____iface", Span::mixed_site());
        let body = self.inner.child_type_init_body(&type_ident, &iface_ident)?;
        let trait_name = format_ident!("{}Impl", name);

        let pred = syn::parse_quote! {
            <#type_ident as #glib::subclass::types::ObjectSubclass>::Type: #glib::IsA<#glib::Object>
        };
        let head = if let Some(mut generics) = self.inner.generics.clone() {
            {
                let where_clause = generics.make_where_clause();
                where_clause.predicates.push(pred);
            }
            let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
            quote! {
                unsafe impl #impl_generics #glib::subclass::types::IsImplementable<#type_ident>
                    for #name #type_generics #where_clause
            }
        } else {
            quote! {
                unsafe impl<#type_ident: #trait_name> #glib::subclass::types::IsImplementable<#type_ident> for #name
                where #pred
            }
        };
        Some(quote! {
            #head {
                fn interface_init(#iface_ident: &mut #glib::Interface<Self>) {
                    let #iface_ident = ::std::convert::AsMut::as_mut(#iface_ident);
                    #body
                }
            }
        })
    }
}

macro_rules! unwrap_or_return {
    ($opt:expr, $ret:expr) => {
        match $opt {
            Some(val) => val,
            None => return $ret,
        }
    };
}

impl ToTokens for InterfaceDefinition {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let glib = unwrap_or_return!(self.inner.glib(), ());
        let module = &self.inner.module;

        let wrapper = self.wrapper();
        let trait_name = self.ext_trait();
        let public_methods = self.inner.public_methods(trait_name.as_ref());
        let is_implementable = self.is_implementable_impl();
        let parent_trait = self
            .parent_trait
            .as_ref()
            .map(|p| p.to_token_stream())
            .unwrap_or_else(|| quote! { #glib::subclass::object::ObjectImpl });
        let virtual_traits = self.inner.virtual_traits(&parent_trait);

        let iface = quote_spanned! { module.span() =>
            #module
            #wrapper
            #public_methods
            #is_implementable
            #virtual_traits
        };
        iface.to_tokens(tokens);
    }
}
