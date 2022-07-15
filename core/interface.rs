use crate::{
    util::{self, Errors},
    Concurrency, TypeBase, TypeDefinition, TypeMode,
};
use darling::{util::{PathList, Flag}, FromMeta};
use heck::ToUpperCamelCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, ToTokens};
use syn::{parse_quote, parse_quote_spanned};

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct Attrs {
    pub name: Option<syn::Ident>,
    pub ns: Option<syn::Ident>,
    pub ext_trait: Option<syn::Ident>,
    pub impl_trait: Option<syn::Ident>,
    pub impl_ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::TypePath>,
    pub wrapper: Option<bool>,
    pub requires: PathList,
    pub sync: Flag,
}

#[derive(Debug)]
pub struct InterfaceOptions(Attrs);

impl InterfaceOptions {
    pub fn parse(tokens: TokenStream, errors: &Errors) -> Self {
        Self(util::parse_list(tokens, errors))
    }
}

#[derive(Debug)]
pub struct InterfaceDefinition {
    pub inner: TypeDefinition,
    pub ns: Option<syn::Ident>,
    pub ext_trait: syn::Ident,
    pub impl_trait: syn::Ident,
    pub impl_ext_trait: syn::Ident,
    pub parent_trait: Option<syn::TypePath>,
    pub wrapper: bool,
    pub requires: Vec<syn::Path>,
}

impl InterfaceDefinition {
    pub fn parse(
        module: syn::ItemMod,
        opts: InterfaceOptions,
        crate_path: syn::Path,
        errors: &Errors,
    ) -> Self {
        let attrs = opts.0;

        let mut inner =
            TypeDefinition::parse(module, TypeBase::Interface, attrs.name, crate_path, errors);

        if attrs.sync.is_some() {
            inner.concurrency = Concurrency::SendSync;
        }

        let name = inner.name.clone();
        Self {
            inner,
            ns: attrs.ns,
            ext_trait: attrs
                .ext_trait
                .unwrap_or_else(|| format_ident!("{}Ext", name)),
            impl_trait: attrs
                .impl_trait
                .unwrap_or_else(|| format_ident!("{}Impl", name)),
            impl_ext_trait: attrs
                .impl_ext_trait
                .unwrap_or_else(|| format_ident!("{}ImplExt", name)),
            parent_trait: attrs.parent_trait,
            wrapper: attrs.wrapper.unwrap_or(true),
            requires: (*attrs.requires).clone(),
        }
    }
    pub fn add_private_items(&mut self, errors: &Errors) {
        let extra = self.extra_private_items();
        self.inner.ensure_items().extend(extra);

        if !self.inner.virtual_methods.is_empty() {
            if let Some(index) = self.inner.properties_item_index {
                let fields = self.inner.type_struct_fields();
                let items = self.inner.ensure_items();
                match &mut items[index] {
                    syn::Item::Struct(s) => match &mut s.fields {
                        syn::Fields::Named(n) => {
                            let fields: syn::FieldsNamed = parse_quote! { {
                                #(pub #fields),*
                            } };
                            n.named.extend(fields.named.into_iter());
                        }
                        f => errors.push_spanned(
                            f,
                            "Interface struct with virtual methods must have named fields",
                        ),
                    },
                    _ => unreachable!(),
                }
            } else if let Some(def) = self.interface_struct_definition() {
                let items = self.inner.ensure_items();
                items.push(syn::Item::Verbatim(def));
            }
        }
    }
    fn extra_private_items(&self) -> Vec<syn::Item> {
        self.inner
            .extra_private_items()
            .into_iter()
            .chain(
                [
                    Some(self.object_interface_impl()),
                    self.interface_struct_definition(),
                    Some(self.is_implementable_impl()),
                    self.inner.virtual_traits(
                        Some(&self.impl_trait),
                        Some(&self.impl_ext_trait),
                        self.parent_trait.as_ref(),
                    ),
                    self.inner.public_methods(Some(&self.ext_trait)),
                ]
                .into_iter()
                .flatten(),
            )
            .map(syn::Item::Verbatim)
            .collect()
    }
    #[inline]
    fn wrapper(&self) -> Option<TokenStream> {
        if !self.wrapper {
            return None;
        }
        let requires = (!self.requires.is_empty()).then(|| {
            let prerequisites = &self.requires;
            quote! { @requires #(#prerequisites),* }
        });
        let mod_name = &self.inner.module.ident;
        let name = &self.inner.name;
        let glib = self.inner.glib();
        let generics = &self.inner.generics;
        let vis = &self.inner.vis;
        Some(quote! {
            #glib::wrapper! {
                #vis struct #name #generics(ObjectInterface<self::#mod_name::#name #generics>) #requires;
            }
        })
    }
    fn interface_init_method(&self) -> Option<TokenStream> {
        let self_ident = syn::Ident::new("self", Span::call_site());
        let body = self.inner.type_init_body(&self_ident);
        let custom = self
            .inner
            .has_method(TypeMode::Subclass, "interface_init")
            .then(|| {
                quote! { Self::interface_init(self); }
            });
        let extra = self.inner.custom_stmts_for("interface_init");
        if body.is_none() && custom.is_none() && extra.is_none() {
            return None;
        }
        Some(quote! {
            fn interface_init(&mut self) {
                #body
                #custom
                #extra
            }
        })
    }
    fn interface_struct_definition(&self) -> Option<TokenStream> {
        if self.inner.properties_item_index.is_some() {
            return None;
        }
        let fields = self.inner.type_struct_fields();
        let name = &self.inner.name;
        let generics = &self.inner.generics;
        let glib = self.inner.glib();
        let vis = &self.inner.inner_vis;
        Some(quote! {
            #[repr(C)]
            #[::std::prelude::v1::derive(Copy, Clone)]
            #vis struct #name #generics {
                pub ____parent_iface: #glib::gobject_ffi::GTypeInterface,
                #(pub #fields),*
            }
        })
    }
    pub fn prerequisites_alias(&self) -> syn::Ident {
        format_ident!("_{}Prerequisites", self.inner.name)
    }
    #[inline]
    fn object_interface_impl(&self) -> TokenStream {
        let glib = self.inner.glib();
        let name = &self.inner.name;
        let head = self.inner.trait_head(
            &parse_quote! { #name },
            quote! {
                #glib::subclass::prelude::ObjectInterface
            },
        );
        let gtype_name = if let Some(ns) = &self.ns {
            format!("{}{}", ns, name)
        } else {
            name.to_string()
        }
        .to_upper_camel_case();
        let prerequisites = self.prerequisites_alias();
        let interface_init = self.interface_init_method();
        let properties = self.inner.properties_method();
        let signals = self.inner.signals_method();
        let type_init = self.inner.method_wrapper("type_init", |ident| {
            parse_quote_spanned! { Span::mixed_site() =>
                fn #ident(type_: &mut #glib::subclass::types::InitializingType<Self>)
            }
        });
        quote! {
            const _: () = {
                #[allow(unused_imports)]
                use #glib;
                #[#glib::object_interface]
                unsafe #head {
                    const NAME: &'static ::std::primitive::str = #gtype_name;
                    type Prerequisites = super::#prerequisites;
                    #interface_init
                    #properties
                    #signals
                    #type_init
                }
            };
        }
    }
    #[inline]
    fn is_implementable_impl(&self) -> TokenStream {
        let glib = self.inner.glib();
        let name = &self.inner.name;
        let trait_name = &self.impl_trait;
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());

        let param = syn::parse_quote! { #type_ident: #trait_name };
        let pred = syn::parse_quote! {
            <#type_ident as #glib::subclass::types::ObjectSubclass>::Type: #glib::IsA<#glib::Object>
        };
        let (_, type_generics, _) = self.inner.generics.split_for_impl();
        let mut generics = self.inner.generics.clone();
        generics.params.push(param);
        {
            let where_clause = generics.make_where_clause();
            where_clause.predicates.push(pred);
        }
        let (impl_generics, _, where_clause) = generics.split_for_impl();
        let head = quote! {
            unsafe impl #impl_generics #glib::subclass::types::IsImplementable<#type_ident>
                for super::#name #type_generics #where_clause
        };
        let iface_ident = syn::Ident::new("____iface", Span::mixed_site());
        let interface_init = self
            .inner
            .child_type_init_body(&type_ident, &iface_ident, trait_name)
            .map(|body| {
                quote! {
                    fn interface_init(#iface_ident: &mut #glib::Interface<Self>) {
                        let #iface_ident = ::std::convert::AsMut::as_mut(#iface_ident);
                        #body
                    }
                }
            });
        quote! {
            #head {
                #interface_init
            }
        }
    }
}

impl ToTokens for InterfaceDefinition {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let vis = &self.inner.vis;
        let module = &self.inner.module;
        let mod_name = &module.ident;

        let wrapper = self.wrapper();
        let ext = &self.ext_trait;
        let use_ext = self
            .inner
            .public_method_definitions(false)
            .next()
            .is_some()
            .then(|| {
                quote! {
                    #[allow(unused_imports)]
                    #vis use #mod_name::#ext;
                }
            });
        let impl_ = &self.impl_trait;
        let use_impl_ext = (!self.inner.virtual_methods.is_empty()).then(|| {
            let impl_ext = &self.impl_ext_trait;
            quote! {
                #[allow(unused_imports)]
                #vis use #mod_name::#impl_ext;
            }
        });
        let requires_ident = self.prerequisites_alias();
        let requires = &self.requires;

        let iface = quote! {
            #module
            #wrapper
            #use_ext
            #[allow(unused_imports)]
            #vis use #mod_name::#impl_;
            #use_impl_ext
            #[doc(hidden)]
            type #requires_ident = (#(#requires,)*);
        };
        iface.to_tokens(tokens);
    }
}
