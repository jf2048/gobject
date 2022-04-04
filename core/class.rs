use crate::{util, Properties, TypeBase, TypeDefinition};
use darling::{
    util::{Flag, PathList, SpannedValue},
    FromMeta,
};
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
    pub parent_trait: Option<syn::Path>,
    pub wrapper: Option<bool>,
    #[darling(rename = "abstract")]
    pub abstract_: SpannedValue<Flag>,
    #[darling(rename = "final")]
    pub final_: SpannedValue<Flag>,
    pub extends: PathList,
    pub implements: PathList,
}

impl Attrs {
    fn validate(&self, errors: &mut Vec<darling::Error>) {
        use crate::validations::*;
        let abstract_ = ("abstract", check_flag(&self.abstract_));
        let final_ = ("final", check_flag(&self.final_));
        only_one([&abstract_, &final_], errors);
    }
}

#[derive(Debug)]
pub struct ClassOptions(Attrs);

impl ClassOptions {
    pub fn parse(tokens: TokenStream, errors: &mut Vec<darling::Error>) -> Self {
        Self(util::parse_list(tokens, errors))
    }
}

#[derive(Debug)]
pub struct ClassDefinition {
    pub inner: TypeDefinition,
    pub ns: Option<syn::Ident>,
    pub ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::Path>,
    pub wrapper: bool,
    pub abstract_: bool,
    pub final_: bool,
    pub extends: Vec<syn::Path>,
    pub implements: Vec<syn::Path>,
}

impl ClassDefinition {
    pub fn parse(
        module: syn::ItemMod,
        opts: ClassOptions,
        crate_ident: syn::Ident,
        errors: &mut Vec<darling::Error>,
    ) -> Self {
        let attrs = opts.0;
        attrs.validate(errors);

        let inner = TypeDefinition::parse(module, TypeBase::Class, crate_ident, errors);

        let mut class = Self {
            inner,
            ns: attrs.ns,
            ext_trait: attrs.ext_trait,
            parent_trait: attrs.parent_trait,
            wrapper: attrs.wrapper.unwrap_or(true),
            abstract_: attrs.abstract_.is_some(),
            final_: attrs.final_.is_some(),
            extends: (*attrs.extends).clone(),
            implements: (*attrs.implements).clone(),
        };

        if let Some(name) = attrs.name {
            class.inner.name = Some(name);
        }
        if class.inner.name.is_none() {
            util::push_error(
                errors,
                class.inner.span(),
                "Class must have a `name = \"...\"` parameter or a #[properties] struct",
            );
        }

        if class.final_ {
            for virtual_method in &class.inner.virtual_methods {
                util::push_error_spanned(
                    errors,
                    &virtual_method.sig,
                    "Virtual method not allowed on final class",
                );
            }
        }

        let extra = class.extra_private_items();

        let (_, items) = class
            .inner
            .module
            .content
            .get_or_insert_with(Default::default);
        items.extend(extra.into_iter());

        class
    }
    fn extra_private_items(&self) -> Vec<syn::Item> {
        let trait_name = self.ext_trait();
        let parent_trait = self.parent_trait.as_ref().map(|p| quote! { #p });

        self.inner
            .extra_private_items()
            .into_iter()
            .chain(
                [
                    self.properties_base_index_definition(),
                    self.object_subclass_impl(),
                    self.object_impl_impl(),
                    self.class_struct_definition(),
                    self.inner.public_methods(trait_name.as_ref()),
                    self.is_subclassable_impl(),
                    self.inner.virtual_traits(parent_trait),
                ]
                .into_iter()
                .filter_map(|t| t),
            )
            .map(syn::Item::Verbatim)
            .collect()
    }
    fn parent_type(&self) -> Option<TokenStream> {
        let glib = self.inner.glib();
        Some(
            self.extends
                .first()
                .map(|p| p.to_token_stream())
                .unwrap_or_else(|| quote! { #glib::Object }),
        )
    }
    #[inline]
    fn wrapper(&self) -> Option<TokenStream> {
        if !self.wrapper {
            return None;
        }
        let mut inherits = Vec::new();
        if !self.extends.is_empty() {
            let extends = &self.extends;
            inherits.push(quote! { @extends #(#extends),* });
        }
        if !self.implements.is_empty() {
            let implements = &self.implements;
            inherits.push(quote! { @implements #(#implements),* });
        }
        let mod_name = &self.inner.module.ident;
        let name = self.inner.name.as_ref()?;
        let glib = self.inner.glib();
        let generics = self.inner.generics.as_ref();
        Some(quote! {
            #glib::wrapper! {
                pub struct #name #generics(ObjectSubclass<self::#mod_name::#name #generics>) #(#inherits),*;
            }
        })
    }
    #[inline]
    fn ext_trait(&self) -> Option<syn::Ident> {
        if self.final_ {
            return None;
        }
        let name = self.inner.name.as_ref()?;
        Some(
            self.ext_trait
                .clone()
                .unwrap_or_else(|| format_ident!("{}Ext", name)),
        )
    }
    fn class_init_method(&self) -> Option<TokenStream> {
        let glib = self.inner.glib();
        let class_ident = syn::Ident::new("____class", Span::mixed_site());
        let body = self.inner.type_init_body(&quote! { #class_ident });
        let custom = self
            .inner
            .has_method("class_init")
            .then(|| quote! { Self::class_init(#class_ident); });
        let extra = self.inner.custom_stmts_for("class_init");
        if body.is_none() && custom.is_none() && extra.is_none() {
            return None;
        }
        Some(quote! {
            fn class_init(#class_ident: &mut <Self as #glib::subclass::types::ObjectSubclass>::Class) {
                #body
                #extra
                #custom
            }
        })
    }
    fn trait_head(&self, ty: &syn::Ident, trait_: TokenStream) -> TokenStream {
        if let Some(generics) = &self.inner.generics {
            let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
            quote! {
                impl #impl_generics #trait_ for #ty #type_generics #where_clause
            }
        } else {
            quote! {
                impl #trait_ for #ty
            }
        }
    }
    fn class_struct_definition(&self) -> Option<TokenStream> {
        let fields = self.inner.type_struct_fields();
        if fields.is_empty() {
            return None;
        }
        let name = self.inner.name.as_ref()?;
        let generics = self.inner.generics.as_ref()?;
        let class_name = format_ident!("{}Class", name);
        let glib = self.inner.glib();
        let parent_class = if self.extends.is_empty() {
            quote! { #glib::gobject_ffi::GObjectClass }
        } else {
            let parent_type = self.parent_type()?;
            quote! {
                <<#parent_type as #glib::Object::ObjectSubclassIs>::Subclass as #glib::subclass::types::ObjectSubclass>::Class
            }
        };
        let class_struct_head = self.trait_head(
            &class_name,
            quote! {
                #glib::subclass::types::ClassStruct
            },
        );
        let deref_head = self.trait_head(
            &class_name,
            quote! {
                ::std::ops::Deref
            },
        );
        let deref_mut_head = self.trait_head(
            &class_name,
            quote! {
                ::std::ops::DerefMut
            },
        );

        Some(quote! {
            #[repr(C)]
            pub struct #class_name #generics {
                pub ____parent_class: #parent_class,
                #(pub #fields),*
            }
            unsafe #class_struct_head {
                type Type = #name #generics;
            }
            #deref_head {
                type Target = #glib::Class<<#name #generics as #glib::subclass::types::ObjectSubclass>::Type>;

                fn deref(&self) -> &<Self as ::std::ops::Deref>::Target {
                    unsafe {
                        &*(self as *const _ as *const <Self as ::std::ops::Deref>::Target)
                    }
                }
            }

            #deref_mut_head {
                fn deref_mut(&mut self) -> &mut <Self as ::std::ops::Deref>::Target {
                    unsafe {
                        &mut *(self as *mut _ as *mut <Self as ::std::ops::Deref>::Target)
                    }
                }
            }
        })
    }
    #[inline]
    fn object_subclass_impl(&self) -> Option<TokenStream> {
        let glib = self.inner.glib();
        let name = self.inner.name.as_ref()?;
        let head = if let Some(generics) = &self.inner.generics {
            let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
            quote! {
                impl #impl_generics #glib::subclass::types::ObjectSubclass
                for #name #type_generics #where_clause
            }
        } else {
            quote! { impl #glib::subclass::types::ObjectSubclass for #name }
        };
        let gtype_name = if let Some(ns) = &self.ns {
            format!("{}{}", ns, name)
        } else {
            name.to_string()
        }
        .to_upper_camel_case();
        let abstract_ = self.abstract_;
        let parent_type = format_ident!("{}ParentType", name);
        let interfaces = format_ident!("{}Interfaces", name);
        let class_struct_type = (!self.inner.virtual_methods.is_empty()).then(|| {
            let class_name = format_ident!("{}Class", name);
            quote! { type Class = #class_name; }
        });
        let class_init = self.class_init_method();
        let instance_init = self.inner.method_wrapper("instance_init", |ident| {
            parse_quote! {
                fn #ident(obj: &#glib::subclass::types::InitializingObject<Self>)
            }
        });
        let type_init = self.inner.method_wrapper("type_init", |ident| {
            parse_quote! {
                fn #ident(type_: &mut #glib::subclass::types::InitializingType<Self>)
            }
        });
        let new = self
            .inner
            .method_wrapper("new", |ident| parse_quote! { fn #ident() -> Self });
        let with_class = self.inner.method_wrapper("with_class", |ident| {
            parse_quote! {
                fn #ident(klass: &<Self as #glib::subclass::types::ObjectSubclass>::Class) -> Self
            }
        });
        Some(quote! {
            #[#glib::object_subclass]
            #head {
                const NAME: &'static ::std::primitive::str = #gtype_name;
                const ABSTRACT: bool = #abstract_;
                type Type = super::#name;
                type ParentType = super::#parent_type;
                type Interfaces = super::#interfaces;
                #class_struct_type
                #class_init
                #instance_init
                #type_init
                #new
                #with_class
            }
        })
    }
    pub(crate) fn properties_base_index_definition(&self) -> Option<TokenStream> {
        if self.inner.properties.is_empty()
            || (!self.inner.has_method("properties") && !self.inner.has_custom_stmts("properties"))
        {
            return None;
        }
        let glib = self.inner.glib();
        Some(quote! {
            #[doc(hidden)]
            static _GENERATED_PROPERTIES_BASE_INDEX: #glib::once_cell::sync::OnceCell<usize>
                = #glib::once_cell::sync::OnceCell::new();
        })
    }
    fn adjust_property_index(&self) -> Option<TokenStream> {
        self.inner.has_method("properties").then(|| {
            quote! {
                let id = id - _GENERATED_PROPERTIES_BASE_INDEX.get().unwrap();
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
    fn set_property_method(&self) -> Option<TokenStream> {
        if self.inner.properties.is_empty() {
            return None;
        }
        let go = &self.inner.crate_ident;
        let glib = self.inner.glib();
        let adjust_index = self.adjust_property_index();
        let extra = self.inner.custom_stmts_for("set_property");
        let set_impls = self
            .inner
            .properties
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| {
                let method = prop
                    .custom_method_path(true)
                    .and_then(|ident| self.inner.find_method(&*ident));
                prop.set_impl(index, method, go)
            });
        let rest = self
            .inner
            .has_method("set_property")
            .then(|| {
                quote! {
                    Self::set_property(self, obj, id, value, pspec)
                }
            })
            .unwrap_or_else(|| Self::unimplemented_property(&glib));
        Some(quote! {
            fn set_property(
                &self,
                obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type,
                id: usize,
                value: &#glib::Value,
                pspec: &#glib::ParamSpec
            ) {
                #adjust_index
                #extra
                #(#set_impls)*
                #rest
            }
        })
    }
    fn property_method(&self) -> Option<TokenStream> {
        if self.inner.properties.is_empty() {
            return None;
        }
        let go = &self.inner.crate_ident;
        let glib = self.inner.glib();
        let adjust_index = self.adjust_property_index();
        let extra = self.inner.custom_stmts_for("property");
        let get_impls = self
            .inner
            .properties
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| {
                let method = prop
                    .custom_method_path(false)
                    .and_then(|ident| self.inner.find_method(&*ident));
                prop.get_impl(index, method, go)
            });
        let rest = self
            .inner
            .has_method("property")
            .then(|| {
                quote! {
                    Self::property(self, obj, id, pspec)
                }
            })
            .unwrap_or_else(|| Self::unimplemented_property(&glib));
        Some(quote! {
            fn property(
                &self,
                obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type,
                id: usize,
                pspec: &#glib::ParamSpec
            ) -> #glib::Value {
                #adjust_index
                #extra
                #(#get_impls)*
                #rest
            }
        })
    }
    #[inline]
    fn object_impl_impl(&self) -> Option<TokenStream> {
        let glib = self.inner.glib();
        let name = self.inner.name.as_ref()?;
        let properties = self.inner.properties_method();
        let signals = self.inner.signals_method();
        let set_property = self.set_property_method();
        let property = self.property_method();
        let constructed = self.inner.method_wrapper("constructed", |ident| {
            parse_quote! {
                fn #ident(&self, obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type)
            }
        });
        let dispose = self.inner.method_wrapper("dispose", |ident| {
            parse_quote! {
                fn #ident(&self, obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type)
            }
        });
        Some(quote! {
            impl #glib::subclass::object::ObjectImpl for #name {
                #properties
                #set_property
                #property
                #signals
                #constructed
                #dispose
            }
        })
    }
    #[inline]
    fn is_subclassable_impl(&self) -> Option<TokenStream> {
        if self.final_ {
            return None;
        }
        let glib = self.inner.glib();
        let name = self.inner.name.as_ref()?;
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());
        let trait_name = format_ident!("{}Impl", name);
        let param = syn::parse_quote! { #type_ident: #trait_name };
        let head = if let Some(generics) = &self.inner.generics {
            let (_, type_generics, _) = generics.split_for_impl();
            let mut generics = generics.clone();
            generics.params.push(param);
            let (impl_generics, _, where_clause) = generics.split_for_impl();
            quote! {
                unsafe impl #impl_generics #glib::subclass::types::IsSubclassable<#type_ident>
                    for super::#name #type_generics #where_clause
            }
        } else {
            quote! {
                unsafe impl<#param> #glib::subclass::types::IsSubclassable<#type_ident> for super::#name
            }
        };
        let class_ident = syn::Ident::new("____class", Span::mixed_site());
        let class_init = self
            .inner
            .child_type_init_body(&type_ident, &class_ident)
            .map(|body| {
                quote! {
                    fn class_init(#class_ident: &mut #glib::Class<Self>) {
                        <Self as #glib::subclass::types::IsSubclassableExt>::parent_class_init::<#type_ident>(
                            #glib::object::Class::upcast_ref_mut(#class_ident)
                        );
                        let #class_ident = ::std::convert::AsMut::as_mut(#class_ident);
                        #body
                    }
                }
            });
        Some(quote! {
            #head {
                #class_init
            }
        })
    }
}

impl ToTokens for ClassDefinition {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let name = match self.inner.name.as_ref() {
            Some(n) => n,
            _ => return,
        };
        let module = &self.inner.module;

        let wrapper = self.wrapper();
        let use_traits = self.ext_trait().map(|ext| {
            let mod_name = &module.ident;
            let impl_ = format_ident!("{}Impl", name);
            let mut use_traits = quote! {
                pub use #mod_name::#ext;
                pub use #mod_name::#impl_;
            };
            if !self.inner.virtual_methods.is_empty() {
                let impl_ext = format_ident!("{}ImplExt", name);
                use_traits.extend(quote! { pub use #mod_name::#impl_ext; });
            }
            use_traits
        });
        let parent_type = self.parent_type().map(|p| {
            let ident = format_ident!("{}ParentType", name);
            quote! { type #ident = #p; }
        });
        let interfaces_ident = format_ident!("{}Interfaces", name);
        let interfaces = &self.implements;
        let interfaces = quote! {
            type #interfaces_ident = (#(#interfaces,)*);
        };

        let class = quote_spanned! { module.span() =>
            #module
            #wrapper
            #use_traits
            #parent_type
            #interfaces
        };
        class.to_tokens(tokens);
    }
}

pub fn derived_class_properties(
    input: &syn::DeriveInput,
    go: &syn::Ident,
    errors: &mut Vec<darling::Error>,
) -> TokenStream {
    let Properties {
        final_type,
        base,
        properties,
        ..
    } = Properties::from_derive_input(input, None, errors);
    let glib = quote! { #go::glib };
    let name = &input.ident;
    let generics = &input.generics;

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let ty = quote! { #name #type_generics };
    let properties_path = quote! { #ty::derived_properties };
    let wrapper_ty = quote! { <#ty as #glib::subclass::types::ObjectSubclass>::Type };
    let trait_name = final_type
        .is_none()
        .then(|| format_ident!("{}PropertiesExt", input.ident));

    let mut items = Vec::new();
    for (index, prop) in properties.iter().enumerate() {
        for item in prop.method_definitions(index, &wrapper_ty, &properties_path, go) {
            items.push(item);
        }
    }

    let public_methods = if let Some(trait_name) = trait_name {
        let type_ident = format_ident!("____Object");
        let mut generics = generics.clone();
        let param = syn::parse_quote! { #type_ident: #glib::IsA<#wrapper_ty> };
        generics.params.push(param);
        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let protos = properties
            .iter()
            .map(|p| p.method_prototypes(go))
            .flatten()
            .collect::<Vec<_>>();

        quote! {
            pub trait #trait_name: 'static {
                #(#protos;)*
            }
            impl #impl_generics #trait_name for #type_ident #where_clause {
                #(#items)*
            }
        }
    } else {
        let final_type = final_type.as_ref().unwrap();
        quote! {
            impl #impl_generics #final_type #type_generics #where_clause {
                #(#items)*
            }
        }
    };

    let defs = properties.iter().map(|p| p.definition(go));
    let access = if base == TypeBase::Class {
        let set_impls = properties
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| prop.set_impl(index, None, go));
        let get_impls = properties
            .iter()
            .enumerate()
            .filter_map(|(index, prop)| prop.get_impl(index, None, go));
        let unimplemented = ClassDefinition::unimplemented_property(&glib);
        Some(quote! {
            fn derived_set_property(
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
            fn derived_get_property(
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
    } else {
        None
    };

    quote! {
        #public_methods
        impl #impl_generics #ty #where_clause {
            fn derived_properties() -> &'static [#glib::ParamSpec] {
                static PROPS: #glib::once_cell::sync::Lazy<::std::vec::Vec<#glib::ParamSpec>> =
                    #glib::once_cell::sync::Lazy::new(|| {
                        vec![#(#defs),*]
                    });
                ::std::convert::AsRef::as_ref(::std::ops::Deref::deref(&PROPS))
            }
            #access
        }
    }
}
