use crate::{
    util::{self, Errors},
    Concurrency, Properties, TypeBase, TypeDefinition, TypeMode,
};
use darling::{
    util::{Flag, PathList, SpannedValue},
    FromMeta,
};
use heck::ToUpperCamelCase;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::{parse_quote, parse_quote_spanned, spanned::Spanned};

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct Attrs {
    pub name: Option<syn::Ident>,
    pub ns: Option<syn::Ident>,
    pub class: Option<syn::Ident>,
    pub ext_trait: Option<syn::Ident>,
    pub impl_trait: Option<syn::Ident>,
    pub impl_ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::TypePath>,
    pub wrapper: Option<bool>,
    #[darling(rename = "abstract")]
    pub abstract_: SpannedValue<Flag>,
    #[darling(rename = "final")]
    pub final_: SpannedValue<Flag>,
    pub extends: PathList,
    pub implements: PathList,
    pub inherits: PathList,
    pub sync: Flag,
}

impl Attrs {
    fn validate(&self, errors: &Errors) {
        use crate::validations::*;
        let abstract_ = ("abstract", check_flag(&self.abstract_));
        let final_ = ("final", check_flag(&self.final_));
        only_one([&abstract_, &final_], errors);
    }
}

#[derive(Debug)]
pub struct ClassOptions(Attrs);

impl ClassOptions {
    pub fn parse(tokens: TokenStream, errors: &Errors) -> Self {
        Self(util::parse_list(tokens, errors))
    }
}

#[derive(Debug)]
pub struct ClassDefinition {
    pub inner: TypeDefinition,
    pub ns: Option<syn::Ident>,
    pub class: syn::Ident,
    pub ext_trait: Option<syn::Ident>,
    pub impl_trait: Option<syn::Ident>,
    pub impl_ext_trait: Option<syn::Ident>,
    pub parent_trait: Option<syn::TypePath>,
    pub wrapper: bool,
    pub abstract_: bool,
    pub final_: bool,
    pub extends: Vec<syn::Path>,
    pub implements: Vec<syn::Path>,
    pub inherits: Vec<syn::Path>,
}

impl ClassDefinition {
    pub fn parse(
        module: syn::ItemMod,
        opts: ClassOptions,
        crate_path: syn::Path,
        errors: &Errors,
    ) -> Self {
        let attrs = opts.0;
        attrs.validate(errors);

        let mut inner =
            TypeDefinition::parse(module, TypeBase::Class, attrs.name, crate_path, errors);

        if attrs.sync.is_some() {
            inner.concurrency = Concurrency::SendSync;
        }

        let name = inner.name.clone();
        let final_ = attrs.final_.is_some();
        let class = Self {
            inner,
            ns: attrs.ns,
            class: attrs
                .class
                .unwrap_or_else(|| format_ident!("{}Class", name)),
            ext_trait: (!final_).then(|| {
                attrs
                    .ext_trait
                    .unwrap_or_else(|| format_ident!("{}Ext", name))
            }),
            impl_trait: (!final_).then(|| {
                attrs
                    .impl_trait
                    .unwrap_or_else(|| format_ident!("{}Impl", name))
            }),
            impl_ext_trait: (!final_).then(|| {
                attrs
                    .impl_ext_trait
                    .unwrap_or_else(|| format_ident!("{}ImplExt", name))
            }),
            parent_trait: attrs.parent_trait,
            wrapper: attrs.wrapper.unwrap_or(true),
            abstract_: attrs.abstract_.is_some(),
            final_,
            extends: (*attrs.extends).clone(),
            implements: (*attrs.implements).clone(),
            inherits: (*attrs.inherits).clone(),
        };

        if class.final_ {
            for virtual_method in &class.inner.virtual_methods {
                errors.push_spanned(
                    &virtual_method.sig,
                    "Virtual method not allowed on final class",
                );
            }
        } else if let Some(extends) = class.extends.first() {
            if class.parent_trait.is_none() {
                errors.push_spanned(
                    extends,
                    "Derivable class must specify `parent_trait` when using `extends`",
                );
            }
        }

        class
    }
    pub fn add_private_items(&mut self) {
        let extra = self.extra_private_items();
        self.inner.ensure_items().extend(extra);
    }
    fn extra_private_items(&self) -> Vec<syn::Item> {
        self.inner
            .extra_private_items()
            .into_iter()
            .chain(
                [
                    self.properties_base_index_definition(),
                    Some(self.object_subclass_impl()),
                    Some(self.object_impl_impl()),
                    self.class_struct_definition(),
                    self.is_subclassable_impl(),
                    self.inner.virtual_traits(
                        self.impl_trait.as_ref(),
                        self.impl_ext_trait.as_ref(),
                        self.parent_trait.as_ref(),
                    ),
                    self.inner.public_methods(self.ext_trait.as_ref()),
                ]
                .into_iter()
                .flatten(),
            )
            .map(syn::Item::Verbatim)
            .collect()
    }
    fn parent_type(&self) -> TokenStream {
        self.extends
            .first()
            .map(|p| p.to_token_stream())
            .unwrap_or_else(|| {
                let glib = self.inner.glib();
                quote! { #glib::Object }
            })
    }
    #[inline]
    fn wrapper(&self) -> Option<TokenStream> {
        if !self.wrapper {
            return None;
        }
        let mut params = Vec::new();
        if !self.extends.is_empty() {
            let extends = &self.extends;
            params.push(quote! { @extends #(#extends),* });
        }
        let mut implements = self
            .implements
            .iter()
            .chain(self.inherits.iter())
            .peekable();
        if implements.peek().is_some() {
            params.push(quote! { @implements #(#implements),* });
        }
        let mod_name = &self.inner.module.ident;
        let name = &self.inner.name;
        let glib = self.inner.glib();
        let generics = &self.inner.generics;
        let vis = &self.inner.vis;
        Some(quote! {
            #glib::wrapper! {
                #vis struct #name #generics(ObjectSubclass<self::#mod_name::#name #generics>) #(#params),*;
            }
        })
    }
    fn class_init_method(&self) -> Option<TokenStream> {
        let glib = self.inner.glib();
        let class_ident = syn::Ident::new("class", Span::mixed_site());
        let body = self.inner.type_init_body(&class_ident);
        let custom = self
            .inner
            .has_method(TypeMode::Subclass, "class_init")
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
    fn class_struct_definition(&self) -> Option<TokenStream> {
        let fields = self.inner.type_struct_fields();
        if fields.is_empty() {
            return None;
        }
        let name = &self.inner.name;
        let generics = &self.inner.generics;
        let class_name = &self.class;
        let glib = self.inner.glib();
        let parent_class = if self.extends.is_empty() {
            quote! { #glib::gobject_ffi::GObjectClass }
        } else {
            let parent_type = self.parent_type_alias();
            quote! {
                <super::#parent_type as #glib::object::ObjectType>::GlibClassType
            }
        };
        let class_name = parse_quote! { #class_name };
        let class_struct_head = self.inner.trait_head(
            &class_name,
            quote! {
                #glib::subclass::types::ClassStruct
            },
        );
        let deref_head = self.inner.trait_head(
            &class_name,
            quote! {
                ::std::ops::Deref
            },
        );
        let deref_mut_head = self.inner.trait_head(
            &class_name,
            quote! {
                ::std::ops::DerefMut
            },
        );
        let vis = &self.inner.inner_vis;

        Some(quote! {
            #[repr(C)]
            #vis struct #class_name #generics {
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
    pub fn parent_type_alias(&self) -> syn::Ident {
        format_ident!("_{}ParentType", self.inner.name)
    }
    pub fn interfaces_alias(&self) -> syn::Ident {
        format_ident!("_{}Interfaces", self.inner.name)
    }
    #[inline]
    fn object_subclass_impl(&self) -> TokenStream {
        let glib = self.inner.glib();
        let name = &self.inner.name;
        let head = self.inner.trait_head(
            &parse_quote! { #name },
            quote! {
                #glib::subclass::types::ObjectSubclass
            },
        );
        let gtype_name = if let Some(ns) = &self.ns {
            format!("{}{}", ns, name)
        } else {
            name.to_string()
        }
        .to_upper_camel_case();
        let abstract_ = self.abstract_;
        let parent_type = self.parent_type_alias();
        let interfaces = self.interfaces_alias();
        let class_name = &self.class;
        let class_struct_type = (!self.inner.virtual_methods.is_empty()).then(|| {
            quote! { type Class = #class_name; }
        });
        let class_init = self.class_init_method();
        let instance_init = self.inner.method_wrapper("instance_init", |ident| {
            parse_quote_spanned! { Span::mixed_site() =>
                fn #ident(obj: &#glib::subclass::types::InitializingObject<Self>)
            }
        });
        let type_init = self.inner.method_wrapper("type_init", |ident| {
            parse_quote_spanned! { Span::mixed_site() =>
                fn #ident(type_: &mut #glib::subclass::types::InitializingType<Self>)
            }
        });
        let new = self.inner.method_wrapper("new", |ident| {
            parse_quote_spanned! { Span::mixed_site() =>
                fn #ident() -> Self
            }
        });
        let with_class = self.inner.method_wrapper("with_class", |ident| {
            parse_quote_spanned! { Span::mixed_site() =>
                fn #ident(klass: &<Self as #glib::subclass::types::ObjectSubclass>::Class) -> Self
            }
        });
        quote! {
            const _: () = {
                #[allow(unused_imports)]
                use #glib;
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
            };
        }
    }
    pub(crate) fn properties_base_index_definition(&self) -> Option<TokenStream> {
        if self.inner.properties.is_empty()
            || (!self.inner.has_method(TypeMode::Subclass, "properties")
                && !self.inner.has_custom_stmts("properties"))
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
    fn adjust_property_index(&self) -> TokenStream {
        if self.inner.has_method(TypeMode::Subclass, "properties") {
            quote_spanned! { Span::mixed_site() =>
                let generated_prop_id = id as i64 - *self::_GENERATED_PROPERTIES_BASE_INDEX.get().unwrap() as i64;
            }
        } else {
            quote_spanned! { Span::mixed_site() =>
                let generated_prop_id = id as i64;
            }

        }
    }
    #[inline]
    fn unimplemented_property(glib: &syn::Path) -> TokenStream {
        quote_spanned! { Span::mixed_site() =>
            ::std::unimplemented!(
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
    #[inline]
    fn find_property_method(&self, ident: &syn::Ident) -> Option<(TypeMode, TypeMode)> {
        if let Some(method) = self.inner.find_method_ident(TypeMode::Wrapper, ident) {
            return Some((
                TypeMode::Wrapper,
                method
                    .sig
                    .receiver()
                    .map(|_| TypeMode::Wrapper)
                    .unwrap_or(TypeMode::Subclass),
            ));
        }
        if let Some(method) = self.inner.find_method_ident(TypeMode::Subclass, ident) {
            return Some((
                TypeMode::Subclass,
                method
                    .sig
                    .receiver()
                    .map(|_| TypeMode::Subclass)
                    .unwrap_or(TypeMode::Wrapper),
            ));
        }
        None
    }
    fn set_property_method(&self) -> Option<TokenStream> {
        if self.inner.properties.is_empty() {
            return None;
        }
        let go = &self.inner.crate_path;
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
                    .and_then(|ident| self.find_property_method(&*ident));
                prop.set_impl(index, method, go)
            });
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let obj_ident = syn::Ident::new("obj", Span::mixed_site());
        let id_ident = syn::Ident::new("id", Span::mixed_site());
        let value_ident = syn::Ident::new("value", Span::mixed_site());
        let pspec_ident = syn::Ident::new("pspec", Span::mixed_site());
        let rest = self
            .inner
            .find_method(TypeMode::Subclass, "set_property")
            .map(|method| {
                quote_spanned! { method.sig.span() =>
                    Self::set_property(#self_ident, #obj_ident, #id_ident, #value_ident, #pspec_ident)
                }
            })
            .unwrap_or_else(|| Self::unimplemented_property(&glib));
        let span = self
            .inner
            .properties_item()
            .map(|i| i.span())
            .unwrap_or_else(Span::call_site);
        Some(quote_spanned! { span =>
            fn set_property(
                &#self_ident,
                #obj_ident: &<Self as #glib::subclass::types::ObjectSubclass>::Type,
                #id_ident: usize,
                #value_ident: &#glib::Value,
                #pspec_ident: &#glib::ParamSpec
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
        let go = &self.inner.crate_path;
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
                    .and_then(|ident| self.find_property_method(&*ident));
                prop.get_impl(index, method, go)
            });
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let obj_ident = syn::Ident::new("obj", Span::mixed_site());
        let id_ident = syn::Ident::new("id", Span::mixed_site());
        let pspec_ident = syn::Ident::new("pspec", Span::mixed_site());
        let rest = self
            .inner
            .find_method(TypeMode::Subclass, "property")
            .map(|method| {
                quote_spanned! { method.sig.span() =>
                    Self::property(#self_ident, #obj_ident, #id_ident, #pspec_ident)
                }
            })
            .unwrap_or_else(|| Self::unimplemented_property(&glib));
        let span = self
            .inner
            .properties_item()
            .map(|i| i.span())
            .unwrap_or_else(Span::call_site);
        Some(quote_spanned! { span =>
            fn property(
                &#self_ident,
                #obj_ident: &<Self as #glib::subclass::types::ObjectSubclass>::Type,
                #id_ident: usize,
                #pspec_ident: &#glib::ParamSpec
            ) -> #glib::Value {
                #adjust_index
                #extra
                #(#get_impls)*
                #rest
            }
        })
    }
    #[inline]
    fn object_impl_impl(&self) -> TokenStream {
        let glib = self.inner.glib();
        let name = &self.inner.name;
        let properties = self.inner.properties_method();
        let signals = self.inner.signals_method();
        let set_property = self.set_property_method();
        let property = self.property_method();
        let constructed = self.inner.method_wrapper("constructed", |ident| {
            parse_quote_spanned! { Span::mixed_site() =>
                fn #ident(&self, obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type)
            }
        });
        let dispose = self.inner.method_wrapper("dispose", |ident| {
            parse_quote_spanned! { Span::mixed_site() =>
                fn #ident(&self, obj: &<Self as #glib::subclass::types::ObjectSubclass>::Type)
            }
        });
        let head = self.inner.trait_head(
            &parse_quote! { #name },
            quote! { #glib::subclass::object::ObjectImpl },
        );
        quote! {
            #head {
                #properties
                #set_property
                #property
                #signals
                #constructed
                #dispose
            }
        }
    }
    #[inline]
    fn is_subclassable_impl(&self) -> Option<TokenStream> {
        if self.final_ {
            return None;
        }
        let glib = self.inner.glib();
        let name = &self.inner.name;
        let trait_name = self.impl_trait.as_ref()?;
        let type_ident = syn::Ident::new("____Object", Span::mixed_site());
        let head = self.inner.trait_head_with_params(
            &parse_quote! { super::#name },
            quote! { #glib::subclass::types::IsSubclassable<#type_ident> },
            Some([parse_quote! { #type_ident: #trait_name }]),
        );
        let class_ident = syn::Ident::new("____class", Span::mixed_site());
        let class_init = self
            .inner
            .child_type_init_body(&type_ident, &class_ident, trait_name)
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
            unsafe #head {
                #class_init
            }
        })
    }
}

impl ToTokens for ClassDefinition {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let vis = &self.inner.vis;
        let module = &self.inner.module;
        let mod_name = &module.ident;

        let wrapper = self.wrapper();
        let use_ext = self.ext_trait.as_ref().and_then(|ext| {
            self.inner
                .public_method_definitions(self.final_)
                .next()
                .is_some()
                .then(|| {
                    quote! {
                        #[allow(unused_imports)]
                        #vis use #mod_name::#ext;
                    }
                })
        });
        let use_impl = self.impl_trait.as_ref().map(|impl_| {
            quote! {
                #[allow(unused_imports)]
                #vis use #mod_name::#impl_;
            }
        });
        let use_impl_ext = self.impl_ext_trait.as_ref().and_then(|impl_ext| {
            (!self.inner.virtual_methods.is_empty()).then(|| {
                quote! {
                    #[allow(unused_imports)]
                    #vis use #mod_name::#impl_ext;
                }
            })
        });
        let parent_type_ident = self.parent_type_alias();
        let parent_type = self.parent_type();
        let interfaces_ident = self.interfaces_alias();
        let interfaces = &self.implements;

        let class = quote! {
            #module
            #wrapper
            #use_ext
            #use_impl
            #use_impl_ext
            #[doc(hidden)]
            type #parent_type_ident = #parent_type;
            #[doc(hidden)]
            type #interfaces_ident = (#(#interfaces,)*);
        };
        class.to_tokens(tokens);
    }
}

pub fn derived_class_properties(
    input: &syn::DeriveInput,
    go: &syn::Path,
    errors: &Errors,
) -> TokenStream {
    let Properties {
        final_type,
        base,
        properties,
        ..
    } = Properties::from_derive_input(input, None, errors);
    let glib: syn::Path = parse_quote! { #go::glib };
    let name = &input.ident;
    let generics = &input.generics;

    let (impl_generics, type_generics, where_clause) = generics.split_for_impl();
    let ty = quote! { #name #type_generics };
    let properties_path = parse_quote! { #ty::derived_properties };
    let wrapper_ty = parse_quote! { <#ty as #glib::subclass::types::ObjectSubclass>::Type };
    let trait_name = final_type
        .is_none()
        .then(|| format_ident!("{}PropertiesExt", input.ident));

    let mut items = Vec::new();
    for (index, prop) in properties.iter().enumerate() {
        for item in
            prop.method_definitions(index, &wrapper_ty, Concurrency::None, &properties_path, go)
        {
            items.push(item);
        }
    }

    let public_methods = if let Some(trait_name) = trait_name {
        let type_ident = format_ident!("____Object");
        let mut generics = generics.clone();
        let param = parse_quote! { #type_ident: #glib::IsA<#wrapper_ty> };
        generics.params.push(param);
        let (impl_generics, _, where_clause) = generics.split_for_impl();

        let protos = properties
            .iter()
            .flat_map(|p| p.method_prototypes(Concurrency::None, go))
            .collect::<Vec<_>>();
        let vis = &input.vis;

        quote! {
            #vis trait #trait_name: 'static {
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
                        ::std::vec![#(#defs),*]
                    });
                ::std::convert::AsRef::as_ref(::std::ops::Deref::deref(&PROPS))
            }
            #access
        }
    }
}
