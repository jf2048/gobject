use crate::{
    util::{self, Errors},
    TypeBase, TypeDefinition, TypeMode,
};
use darling::{util::PathList, FromMeta};
use heck::ToUpperCamelCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::{parse_quote, spanned::Spanned};

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct Attrs {
    pub name: Option<syn::Ident>,
    pub ns: Option<syn::Ident>,
    pub ext_trait: Option<syn::Ident>,
    pub impl_trait: Option<syn::Ident>,
    pub impl_ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::Path>,
    pub wrapper: Option<bool>,
    pub requires: PathList,
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
    pub ext_trait: Option<syn::Ident>,
    pub impl_trait: Option<syn::Ident>,
    pub impl_ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::Path>,
    pub wrapper: bool,
    pub requires: Vec<syn::Path>,
}

impl InterfaceDefinition {
    pub fn parse(
        module: syn::ItemMod,
        opts: InterfaceOptions,
        crate_ident: syn::Ident,
        errors: &Errors,
    ) -> Self {
        let attrs = opts.0;

        let inner =
            TypeDefinition::parse(module, TypeBase::Interface, attrs.name, crate_ident, errors);

        let name = inner.name.clone();
        let mut iface = Self {
            inner,
            ns: attrs.ns,
            ext_trait: attrs
                .ext_trait
                .or_else(|| name.as_ref().map(|n| format_ident!("{}Ext", n))),
            impl_trait: attrs
                .impl_trait
                .or_else(|| name.as_ref().map(|n| format_ident!("{}Impl", n))),
            impl_ext_trait: attrs
                .impl_ext_trait
                .or_else(|| name.as_ref().map(|n| format_ident!("{}ImplExt", n))),
            parent_trait: attrs.parent_trait,
            wrapper: attrs.wrapper.unwrap_or(true),
            requires: (*attrs.requires).clone(),
        };

        if iface.inner.name.is_none() {
            errors.push(
                iface.inner.span(),
                "Interface must have a `name = \"...\"` parameter, a struct, or an impl",
            );
        }

        let extra = iface.extra_private_items();

        iface.inner.ensure_items().extend(extra.into_iter());

        if !iface.inner.virtual_methods.is_empty() {
            if let Some(index) = iface.inner.properties_item_index {
                let fields = iface.inner.type_struct_fields();
                let items = iface.inner.ensure_items();
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
            } else if let Some(def) = iface.interface_struct_definition() {
                let items = iface.inner.ensure_items();
                items.push(syn::Item::Verbatim(def));
            }
        }

        iface
    }
    fn extra_private_items(&self) -> Vec<syn::Item> {
        self.inner
            .extra_private_items()
            .into_iter()
            .chain(
                [
                    self.object_interface_impl(),
                    self.interface_struct_definition(),
                    self.inner.public_methods(self.ext_trait.as_ref()),
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
        let name = self.inner.name.as_ref()?;
        let glib = self.inner.glib();
        let generics = self.inner.generics.as_ref();
        let vis = &self.inner.vis;
        Some(quote! {
            #glib::wrapper! {
                #vis struct #name #generics(ObjectInterface<self::#mod_name::#name #generics>) #requires;
            }
        })
    }
    fn interface_init_method(&self) -> Option<TokenStream> {
        let body = self.inner.type_init_body(&quote! { self });
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
        let name = self.inner.name.as_ref()?;
        let generics = self.inner.generics.as_ref()?;
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
    pub fn prerequisites_alias(&self) -> Option<syn::Ident> {
        Some(format_ident!("_{}Prerequisites", self.inner.name.as_ref()?))
    }
    #[inline]
    fn object_interface_impl(&self) -> Option<TokenStream> {
        let glib = self.inner.glib();
        let name = self.inner.name.as_ref()?;
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
        let prerequisites = self.prerequisites_alias()?;
        let interface_init = self.interface_init_method();
        let properties = self.inner.properties_method();
        let signals = self.inner.signals_method();
        let type_init = self.inner.method_wrapper("type_init", |ident| {
            parse_quote! {
                fn #ident(type_: &mut #glib::subclass::types::InitializingType<Self>)
            }
        });
        Some(quote! {
            const _: () = {
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
        })
    }
    #[inline]
    fn is_implementable_impl(&self) -> Option<TokenStream> {
        let glib = self.inner.glib();
        let name = self.inner.name.as_ref()?;
        let trait_name = self.impl_trait.as_ref()?;
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());

        let param = syn::parse_quote! { #type_ident: #trait_name };
        let pred = syn::parse_quote! {
            <#type_ident as #glib::subclass::types::ObjectSubclass>::Type: #glib::IsA<#glib::Object>
        };
        let head = if let Some(generics) = &self.inner.generics {
            let (_, type_generics, _) = generics.split_for_impl();
            let mut generics = generics.clone();
            generics.params.push(param);
            {
                let where_clause = generics.make_where_clause();
                where_clause.predicates.push(pred);
            }
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            quote! {
                unsafe impl #impl_generics #glib::subclass::types::IsImplementable<#type_ident>
                    for #name #type_generics #where_clause
            }
        } else {
            quote! {
                unsafe impl<#param> #glib::subclass::types::IsImplementable<#type_ident> for #name
                where #pred
            }
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
        Some(quote! {
            #head {
                #interface_init
            }
        })
    }
}

impl ToTokens for InterfaceDefinition {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let module = &self.inner.module;

        let wrapper = self.wrapper();
        let is_implementable = self.is_implementable_impl();
        let use_trait = self.ext_trait.as_ref().and_then(|ext| {
            self.inner
                .public_method_definitions(false)
                .and_then(|mut i| i.next())
                .is_some()
                .then(|| {
                    let mod_name = &module.ident;
                    let vis = &self.inner.vis;
                    quote! { #vis use #mod_name::#ext; }
                })
        });
        let parent_trait = self.parent_trait.as_ref().map(|p| quote! { #p });
        let virtual_traits = self.inner.virtual_traits(
            self.impl_trait.as_ref(),
            self.impl_ext_trait.as_ref(),
            parent_trait,
        );
        let requires = self.prerequisites_alias().map(|ident| {
            let requires = &self.requires;
            quote! {
                #[doc(hidden)]
                type #ident = (#(#requires,)*);
            }
        });

        let iface = quote_spanned! { module.span() =>
            #module
            #wrapper
            #is_implementable
            #use_trait
            #virtual_traits
            #requires
        };
        iface.to_tokens(tokens);
    }
}
