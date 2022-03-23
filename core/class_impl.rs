use crate::{property::Property, type_definition::TypeDefinition, util};
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
pub struct Options {
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

impl Options {
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
    fn ty(&self, def: &TypeDefinition) -> syn::Type {
        todo!()
    }
    fn parent_type(&self) -> Option<TokenStream> {
        self.extends.first().map(|p| p.to_token_stream())
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
        if self.final_.is_some() {
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
        def: &TypeDefinition,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        let name = self.name.as_ref()?;
        let gtype_name = if let Some(ns) = &self.ns {
            format!("{}_{}", ns, name)
        } else {
            name.to_string()
        }
        .to_upper_camel_case();
        let abstract_ = self.abstract_.is_some();
        let parent_type = self
            .parent_type()
            .unwrap_or_else(|| quote! { #glib::Object });
        let interfaces = &self.implements;
        let class_init = def
            .custom_method("class_init")
            .or_else(|| def.set_default_vtable().map(|vt| Self::class_init_method("class_init", vt)));
        let instance_init = def.custom_method("instance_init");
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
        method_name: &str,
        props: &[Property],
        properties_path: &TokenStream,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        if props.is_empty() {
            return None;
        }
        let set_impls = props
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| prop.set_impl(index, go));
        let glib = quote! { #go::glib };
        let method_name = format_ident!("{}", method_name);
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
        method_name: &str,
        props: &[Property],
        properties_path: &TokenStream,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        if props.is_empty() {
            return None;
        }
        let get_impls = props
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| prop.get_impl(index, go));
        let glib = quote! { #go::glib };
        let method_name = format_ident!("{}", method_name);
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
    fn method_path(method: &str, def: &TypeDefinition, ty: &syn::Type, glib: &TokenStream) -> TokenStream {
        def.has_custom_method(method) {
            let method = format_ident!("derived_{}", method);
            quote! { #ty::#method }
        } else {
            let method = format_ident!("{}", method);
            quote! { <#ty as #glib::subclass::object::ObjectImpl>::#method }
        }
    }
    #[inline]
    fn object_impl_impl(&self, def: &TypeDefinition, go: &syn::Ident) -> Option<TokenStream> {
        let glib = quote! { #go::glib };
        let name = self.name.as_ref()?;
        let properties = def
            .custom_method("properties")
            .or_else(|| def.properties_method("properties", go));
        let signals = def
            .custom_method("signals")
            .or_else(|| def.signals_method("signals", &glib));
        let properties_path = if def.has_custom_method("properties") {
            quote! { #name::derived_property }
        } else {
            quote! { <#name as #glib::subclass::object::ObjectImpl>::properties }
        };
        let set_property = def
            .custom_method("set_property")
            .or_else(|| Self::set_property_method("set_property", &def.properties, &properties_path, go));
        let property = def
            .custom_method("property")
            .or_else(|| Self::property_method("property", &def.properties, &properties_path, go));
        let extra = def.custom_methods(&["constructed", "dispose"]);
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
}

pub(crate) fn class_impl(
    mut opts: Options,
    module: syn::ItemMod,
    errors: &mut Vec<darling::Error>,
) -> TokenStream {
    let (def, mut module) = TypeDefinition::new(
        module,
        false,
        &["properties", "signals", "set_property", "property", "constructed", "dispose", "class_init", "instance_init"],
        errors
    );
    opts.normalize(&def);
    opts.validate(&def, errors);
    let go = util::crate_ident();
    let glib = quote! { #go::glib };
    let ty = opts.ty(&def);

    let (_, items) = module.content.get_or_insert_with(Default::default);
    if let Some(object_subclass_impl) = opts.object_subclass_impl(&def, &glib) {
        items.push(syn::Item::Verbatim(object_subclass_impl));
    }
    if let Some(object_impl_impl) = opts.object_impl_impl(&def, &go) {
        items.push(syn::Item::Verbatim(object_impl_impl));
    }
    // TODO add private 'derived_*' impls

    let wrapper = opts.wrapper(&module.ident, &glib);
    let public_methods = opts.name.as_ref().map(|name| {
        let trait_name = opts.ext_trait();
        def.public_methods(name, &ty, trait_name.as_ref(), false, &go)
    });
    let is_subclassable = def.set_subclassed_vtable().map(|vt| {
        opts.is_subclassable_impl(
            &vt,
            def.generics.clone(),
            &glib
        )
    });
    let virtual_traits = opts.name.as_ref().and_then(|name| {
        let parent_trait = opts.parent_trait.as_ref()
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
