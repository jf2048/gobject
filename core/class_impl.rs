use crate::{property::Property, type_definition::TypeDefinition, util, TypeDefinitionParser, TypeDefinitionParser};
use darling::{
    util::{Flag, PathList, SpannedValue},
    FromMeta,
};
use heck::ToUpperCamelCase;
use proc_macro2::TokenStream;
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
    #[darling(rename = "abstract")]
    pub abstract_: SpannedValue<Flag>,
    #[darling(rename = "final")]
    pub final_: SpannedValue<Flag>,
    pub extends: PathList,
    pub implements: PathList,
}

pub struct ClassOptions(Attrs);

impl ClassOptions {
    pub fn parse(tokens: TokenStream, errors: &mut Vec<darling::Error>) -> Self {
        Self(util::parse_list(tokens, errors))
    }
}

impl Attrs {
    fn normalize(&mut self, def: &TypeDefinition) {
        if self.name.is_none() {
            self.name = def.name().cloned();
        }
    }
    fn validate(&self, def: &TypeDefinition, errors: &mut Vec<darling::Error>) {
        use crate::validations::*;

        if self.name.is_none() {
            util::push_error(
                errors,
                def.span(),
                "Class must have a `name = \"...\"` parameter or a #[properties] struct",
            );
        }
        let abstract_ = ("abstract", check_flag(&self.abstract_));
        let final_ = ("final", check_flag(&self.final_));
        only_one([&abstract_, &final_], errors);
    }
}

pub struct ClassDefinition {
    pub inner: TypeDefinition,
    pub ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::Path>,
    pub wrapper: bool,
    pub abstract_: bool,
    pub final_: bool,
    pub extends: Vec<syn::Path>,
    pub implements: Vec<syn::Path>,
}

impl ClassDefinition {
    pub fn type_parser() -> TypeDefinitionParser {
        *TypeDefinitionParser::new()
            .add_custom_method("properties")
            .add_custom_method("signals")
            .add_custom_method("set_property")
            .add_custom_method("property")
            .add_custom_method("constructed")
            .add_custom_method("dispose")
            .add_custom_method("class_init")
            .add_custom_method("instance_init")
    }
    pub fn from_mod(
        module: &mut syn::ItemMod,
        def: TypeDefinition,
        opts: ClassOptions,
        errors: &mut Vec<darling::Error>,
    ) -> Self {
        let mut attrs = opts.0;
        attrs.normalize(&def);
        attrs.validate(&def, errors);
        Self {
            inner: def,
            ext_trait: attrs.ext_trait,
            parent_trait: attrs.parent_trait,
            wrapper: attrs.wrapper.unwrap_or(true),
            abstract_: attrs.abstract_.is_some(),
            final_: attrs.final_.is_some(),
            extends: FromIterator::from_iter(attrs.extends.into_iter()),
            implements: FromIterator::from_iter(attrs.implements.into_iter()),
        }
    }
    fn name(&self) -> Option<syn::Ident> {
        todo!()
    }
    fn ty(&self) -> syn::Type {
        todo!()
    }
    fn parent_type(&self) -> Option<TokenStream> {
        self.extends.first().map(|p| p.to_token_stream())
    }
    #[inline]
    fn wrapper(&self, mod_ident: &syn::Ident, glib: &TokenStream) -> Option<TokenStream> {
        if !self.wrapper {
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
        if self.final_ {
            return None;
        }
        let name = self.name.as_ref()?;
        Some(
            self.ext_trait
                .clone()
                .unwrap_or_else(|| format_ident!("{}Ext", name)),
        )
    }
    fn class_init_method(
        method_name: &str,
        set_vtable: TokenStream,
    ) -> TokenStream {
        let method_name = format_ident!("{}", method_name);
        todo!("signal overrides, extension bits");
        quote! {
            fn #method_name(klass: &mut Self::Class) {
                #set_vtable
            }
        }
    }
    #[inline]
    fn object_subclass_impl(
        &self,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        let name = self.name()?;
        let gtype_name = if let Some(ns) = &self.ns {
            format!("{}_{}", ns, name)
        } else {
            name.to_string()
        }
        .to_upper_camel_case();
        let abstract_ = self.abstract_;
        let parent_type = self
            .parent_type()
            .unwrap_or_else(|| quote! { #glib::Object });
        let interfaces = &self.implements;
        let class_init = self
            .inner
            .custom_method("class_init")
            .or_else(|| def.set_default_vtable().map(|vt| Self::class_init_method("class_init", vt)));
        let instance_init = self.inner.custom_method("instance_init");
        Some(quote! {
            #[#glib::object_subclass]
            impl #glib::subclass::types::ObjectSubclass for #name {
                const NAME: &'static ::std::primitive::str = #gtype_name;
                const ABSTRACT: bool = #abstract_;
                type Type = super::#name;
                type ParentType = #parent_type;
                type Interfaces = (#(#interfaces,)*);
                #class_init
                #instance_init
            }
        })
    }
    fn unimplemented_property(glib: &TokenStream) -> TokenStream {
        quote! {
            unimplemented!(
                "invalid property id {} for \"{}\" of type '{}' in '{}'",
                id,
                pspec.name(),
                pspec.type_().name(),
                <<Self as #glib::subclass::types::ObjectSubclass>::Type as #glib::object::ObjectExt>::type_(
                    obj
                ).name()
            )
        }
    }
    fn set_property_method(
        &self,
        method_name: &str,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        if self.inner.properties.is_empty() {
            return None;
        }
        let set_impls = self
            .inner
            .properties
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| prop.set_impl(index, go));
        let glib = quote! { #go::glib };
        let method_name = format_ident!("{}", method_name);
        let properties_path = self.properties_path(&glib)?;
        let unimplemented = Self::unimplemented_property(&glib);
        Some(quote! {
            fn #method_name(
                &self,
                obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type,
                id: usize,
                value: &#glib::Value,
                pspec: &#glib::ParamSpec
            ) {
                let properties = #properties_path();
                #(#set_impls)*
                #unimplemented
            }
        })
    }
    fn property_method(
        &self,
        method_name: &str,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        if self.inner.properties.is_empty() {
            return None;
        }
        let get_impls = self
            .inner
            .properties
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| prop.get_impl(index, go));
        let glib = quote! { #go::glib };
        let method_name = format_ident!("{}", method_name);
        let properties_path = self.properties_path(&glib)?;
        let unimplemented = Self::unimplemented_property(&glib);
        Some(quote! {
            fn #method_name(
                &self,
                obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type,
                id: usize,
                pspec: &#glib::ParamSpec
            ) -> #glib::Value {
                let properties = #properties_path();
                #(#get_impls)*
                #unimplemented
            }
        })
    }
    fn method_path(&self, method: &str, glib: &TokenStream) -> TokenStream {
        self.inner.has_custom_method(method) {
            let method = format_ident!("derived_{}", method);
            quote! { #ty::#method }
        } else {
            let method = format_ident!("{}", method);
            quote! { <#ty as #glib::subclass::object::ObjectImpl>::#method }
        }
    }
    fn properties_path(&self, glib: &TokenStream) -> Option<TokenStream> {
        let name = self.name()?;
        Some(if self.inner.has_custom_method("properties") {
            quote! { #name::derived_property }
        } else {
            quote! { <#name as #glib::subclass::object::ObjectImpl>::properties }
        })
    }
    #[inline]
    fn object_impl_impl(&self, go: &syn::Ident) -> Option<TokenStream> {
        let glib = quote! { #go::glib };
        let name = self.name()?;
        let properties = self
            .inner
            .custom_method("properties")
            .or_else(|| self.inner.properties_method("properties", go));
        let signals = self
            .inner
            .custom_method("signals")
            .or_else(|| self.inner.signals_method("signals", &glib));
        let set_property = self
            .inner
            .custom_method("set_property")
            .or_else(|| self.set_property_method("set_property", go));
        let property = self
            .inner
            .custom_method("property")
            .or_else(|| self.property_method("property", go));
        let extra = self.inner.custom_methods(&["constructed", "dispose"]);
        Some(quote! {
            impl #glib::subclass::object::ObjectImpl for #name {
                #properties
                #set_property
                #property
                #signals
                #extra
            }
        })
    }
    #[inline]
    fn is_subclassable_impl(&self, set_vtable: &TokenStream, generics: Option<syn::Generics>, glib: &TokenStream) -> Option<TokenStream> {
        let name = self.name.as_ref()?;
        let trait_name = format_ident!("{}Impl", name);
        let type_ident = format_ident!("____Object");
        let impl_stmt = if let Some(generics) = generics {
            let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
            quote! {
                unsafe impl #impl_generics #glib::subclass::types::IsSubclassable<#type_ident>
                    for #name #type_generics #where_clause
            }
        } else {
            quote! {
                unsafe impl<#type_ident: #trait_name> #glib::subclass::types::IsSubclassable<#type_ident> for #name
            }
        };
        Some(quote! {
            #impl_stmt {
                fn class_init(class: &mut #glib::Class<Self>) {
                    Self::parent_class_init::<T>(#glib::Cast::upcast_ref_mut(class));
                    let klass = ::std::convert::AsMut::as_mut(class);
                    #set_vtable
                }
            }
        })
    }
    pub fn to_tokens(&self, go: &syn::Ident) -> TokenStream {
        let glib = quote! { #go::glib };
        let ty = self.ty();

        let (_, items) = module.content.get_or_insert_with(Default::default);
        if let Some(object_subclass_impl) = self.object_subclass_impl(&glib) {
            items.push(syn::Item::Verbatim(object_subclass_impl));
        }
        if let Some(object_impl_impl) = self.object_impl_impl(&go) {
            items.push(syn::Item::Verbatim(object_impl_impl));
        }
        // TODO add private 'derived_*' impls

        let wrapper = self.wrapper(&module.ident, &glib);
        let public_methods = self.name.as_ref().map(|name| {
            let trait_name = self.ext_trait();
            def.public_methods(name, &ty, trait_name.as_ref(), false, &go)
        });
        let is_subclassable = def.set_subclassed_vtable().map(|vt| {
            self.is_subclassable_impl(
                &vt,
                def.generics.clone(),
                &glib
            )
        });
        let virtual_traits = self.name.as_ref().and_then(|name| {
            let parent_trait = self.parent_trait.as_ref()
                .map(|p| p.to_token_stream())
                .unwrap_or_else(|| quote! { #glib::subclass::object::ObjectImpl });
            def.virtual_traits(&module.ident, name, &parent_trait, &ty, &glib)
        });

        quote_spanned! { module.span() =>
            #module
            #wrapper
            #public_methods
            #is_subclassable
            #virtual_traits
        }
    }

}
