use darling::{
    util::{Flag, PathList, SpannedValue},
    FromAttributes, FromMeta,
};
use gobject_core::{
    util, validations, PropertyOverride, PropertyPermission, PropertyStorage, TypeBase,
    TypeDefinition,
};
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use syn::{parse_quote, spanned::Spanned};

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(variant))]
struct VariantAttrs {
    pub to: Flag,
    pub from: Flag,
    pub dict: SpannedValue<Flag>,
    pub skip_parent: SpannedValue<Flag>,
    pub child_types: PathList,
}

#[derive(Debug, Default, FromAttributes)]
#[darling(default, attributes(variant))]
struct VariantFieldAttrs {
    pub skip: SpannedValue<Flag>,
    pub skip_to: SpannedValue<Flag>,
    pub skip_from: SpannedValue<Flag>,
    pub with: Option<syn::Path>,
    pub variant_type: Option<syn::Path>,
    pub variant_type_str: Option<syn::LitStr>,
    pub to: Option<syn::Path>,
    pub from: Option<syn::Path>,
}

impl VariantFieldAttrs {
    fn validate(&self, errors: &util::Errors) {
        let skip = ("skip", validations::check_flag(&self.skip));
        let skip_to = ("skip_to", validations::check_flag(&self.skip_to));
        let skip_from = ("skip_from", validations::check_flag(&self.skip_from));
        let with = ("with", validations::check_spanned(&self.with));
        let variant_type = (
            "variant_type",
            validations::check_spanned(&self.variant_type),
        );
        let variant_type_str = (
            "variant_type_str",
            validations::check_spanned(&self.variant_type_str),
        );
        let to = ("to", validations::check_spanned(&self.to));
        let from = ("from", validations::check_spanned(&self.from));
        validations::only_one([&skip, &skip_to], errors);
        validations::only_one([&skip, &skip_from], errors);
        validations::only_one([&with, &to], errors);
        validations::only_one([&with, &from], errors);
        validations::only_one([&variant_type, &variant_type_str], errors);
    }
}

pub(crate) fn extend_variant(
    def: &mut TypeDefinition,
    final_: bool,
    abstract_: bool,
    parent_type: Option<&syn::Path>,
    ext_trait: Option<&syn::Ident>,
    errors: &util::Errors,
) {
    let attr = def
        .properties_item_mut()
        .and_then(|item| util::extract_attrs(&mut item.attrs, "variant"))
        .map(|attrs| {
            let span = attrs
                .iter()
                .map(|a| a.span())
                .reduce(|a, b| a.join(b).unwrap_or(a))
                .unwrap_or_else(Span::call_site);
            let attrs = util::parse_attributes::<VariantAttrs>(&attrs, errors);
            if attrs.to.is_none() && attrs.from.is_none() {
                errors.push(
                    span,
                    "Must have at least one of these attributes: `to`, `from`",
                );
            }
            attrs
        })
        .unwrap_or_default();
    let to = attr.to.is_some();
    let from = attr.from.is_some();
    let skip_parent = (*attr.skip_parent).is_some();
    let child_types = attr.child_types;

    if !to && !from {
        return;
    }

    let storages = def
        .properties
        .iter()
        .map(|p| p.storage.clone())
        .collect::<Vec<_>>();
    if let Some(item) = def.properties_item_mut() {
        for storage in storages {
            let field = match &storage {
                PropertyStorage::NamedField(ident) => item
                    .fields
                    .iter_mut()
                    .find(|f| f.ident.as_ref() == Some(ident)),
                PropertyStorage::UnnamedField(id) => item.fields.iter_mut().nth(*id),
                _ => None,
            };
            if let Some(f) = field {
                util::extract_attrs(&mut f.attrs, "variant");
            }
        }
    }

    let go = &def.crate_path;
    let glib: syn::Path = parse_quote! { #go::glib };
    let sub_ty = &def.name;
    let wrapper_ty = parse_quote! { super::#sub_ty };

    let props = def
        .properties
        .iter()
        .filter_map(|prop| {
            if matches!(&prop.override_, Some(PropertyOverride::Class(_))) {
                return None;
            }
            let mut attrs = prop.field.attrs.clone();
            let attrs = util::extract_attrs(&mut attrs, "variant")
                .map(|attrs| {
                    let attrs = util::parse_attributes::<VariantFieldAttrs>(&attrs, errors);
                    attrs.validate(errors);
                    attrs
                })
                .unwrap_or_default();
            attrs.skip.is_none().then(|| (attrs, prop))
        })
        .collect::<Vec<_>>();

    if attr.dict.is_none() && from && to {
        for (attrs, prop) in &props {
            let has_to = prop.get.is_allowed() && attrs.skip_to.is_none();
            let has_from = prop.set.is_allowed() && attrs.skip_from.is_none();
            if has_to != has_from {
                errors.push(
                    prop.span(),
                    "Property for variant differs in readability/writability. \
                    Try using #[variant(skip)] on the property, \
                    or #[variant(dict)] on the class",
                );
            }
        }
    }

    let parent_name = attr
        .dict
        .and(parent_type)
        .and((!skip_parent).then(|| ()))
        .map(|_| {
            let mut parent_name = String::from("parent");
            while props.iter().any(|(_, p)| {
                let mut is_named = false;
                let name_taken = p
                    .field
                    .attrs
                    .iter()
                    .any(|a| has_name(a, &parent_name, &mut is_named));
                name_taken || (!is_named && p.getter_name() == parent_name)
            }) {
                parent_name.insert(0, '_');
            }
            syn::Ident::new(&parent_name, Span::mixed_site())
        });

    let static_variant_type = if attr.dict.is_some() {
        quote! { ::std::borrow::Cow::Borrowed(#glib::VariantTy::VARDICT) }
    } else {
        let builder_ident = syn::Ident::new("builder", Span::mixed_site());
        let parent_type = (!skip_parent).then(|| parent_type).flatten().map(|ty| quote! {
            #builder_ident.append(<#ty as #go::ParentStaticVariantType>::parent_static_variant_type().as_str());
        });
        let prop_types = props.iter().filter_map(|(attr, prop)| {
            let prop = *prop;
            if !prop.get.is_allowed() || attr.skip_to.is_some() {
                return None;
            }
            if !prop.set.is_allowed() || attr.skip_from.is_some() {
                return None;
            }
            let ty = if let Some(type_str) = attr.variant_type_str.as_ref() {
                quote_spanned! { type_str.span() =>
                    #glib::VariantTy::new(#type_str).unwrap().as_str()
                }
            } else if let Some(path) = attr.variant_type.as_ref() {
                let ty_ident = syn::Ident::new("ty", Span::mixed_site());
                quote_spanned! { path.span() => ({
                    let #ty_ident: ::std::borrow::Cow<'static, #glib::VariantTy> = #path();
                    #ty_ident
                }).as_str() }
            } else if let Some(with) = attr.with.as_ref() {
                let ty_ident = syn::Ident::new("ty", Span::mixed_site());
                quote_spanned! { with.span() => ({
                    let #ty_ident: ::std::borrow::Cow<'static, #glib::VariantTy> = #with::static_variant_type();
                    #ty_ident
                }).as_str() }
            } else {
                let ty = prop.store_type(go);
                quote_spanned! { prop.span() =>
                    <#ty as #glib::StaticVariantType>::static_variant_type().as_str()
                }
            };
            Some(quote_spanned! { prop.span() =>
                #builder_ident.append(#ty);
            })
        });
        quote! {
            let mut #builder_ident = <#glib::GStringBuilder as ::std::default::Default>::default();
            #builder_ident.append_c('(');
            #parent_type
            #(#prop_types)*
            #builder_ident.append_c(')');
            ::std::borrow::Cow::Owned(#glib::VariantType::from_string(
                #builder_ident.into_string()
            ).unwrap())
        }
    };
    let parent_static_type = (!final_).then(|| {
        let type_head = def.trait_head(&wrapper_ty, quote! { #go::ParentStaticVariantType });
        quote! {
            #type_head {
                fn parent_static_variant_type() -> ::std::borrow::Cow<'static, #glib::VariantTy> {
                    #static_variant_type
                }
            }
        }
    });
    let static_variant_type = if !final_ && !child_types.is_empty() {
        quote! { ::std::borrow::Cow::Borrowed(unsafe {
            #glib::VariantTy::from_str_unchecked("(sv)")
        }) }
    } else {
        static_variant_type
    };
    let type_head = def.trait_head(&wrapper_ty, quote! { #glib::StaticVariantType });
    let static_variant_type = quote! {
        #type_head {
            fn static_variant_type() -> ::std::borrow::Cow<'static, #glib::VariantTy> {
                #static_variant_type
            }
        }
        #parent_static_type
    };

    let to_variant =
        to.then(|| {
            let self_ident = syn::Ident::new("self", Span::mixed_site());
            let parent_field = (!skip_parent)
                .then(|| parent_type)
                .flatten()
                .map(|ty| {
                    quote! {
                        #go::ToParentVariant::to_parent_variant(
                            #glib::Cast::upcast_ref::<#ty>(#self_ident)
                        )
                    }
                })
                .into_iter();
            let write_fields = props.iter().filter_map(|(attr, prop)| {
                let prop = *prop;
                if !prop.get.is_allowed() || attr.skip_to.is_some() {
                    return None;
                }
                let name = prop.getter_name();
                let getter = match &prop.get {
                    PropertyPermission::Allow | PropertyPermission::AllowCustomDefault => {
                        let path = ext_trait
                            .map(|t| quote! { #t })
                            .unwrap_or_else(|| quote! { #wrapper_ty });
                        quote! { #path::#name(#self_ident) }
                    }
                    PropertyPermission::AllowNoMethod => {
                        let pname = prop.name.to_string();
                        quote! {
                            #glib::prelude::ObjectExt::property(#self_ident, #pname)
                        }
                    }
                    PropertyPermission::AllowCustom(p) => quote! { #p(#self_ident) },
                    _ => unreachable!(),
                };
                Some(if let Some(with) = &attr.with {
                    quote_spanned! { with.span() =>
                        #with::to_variant(&#getter)
                    }
                } else if let Some(to_variant) = &attr.to {
                    quote_spanned! { to_variant.span() =>
                        #to_variant(&#getter)
                    }
                } else {
                    quote! { #glib::ToVariant::to_variant(&#getter) }
                })
            });

            let fields = parent_field.chain(write_fields);
            let create_variant =
                if attr.dict.is_some() {
                    let dict_ident = syn::Ident::new("dict", Span::mixed_site());
                    let parent_name = parent_name.as_ref().map(|p| p.to_string());
                    let field_names = props.iter().filter_map(|(attr, prop)| {
                        let prop = *prop;
                        if !prop.get.is_allowed() || attr.skip_to.is_some() {
                            return None;
                        }
                        Some(prop.name.to_string())
                    });
                    let fields = parent_name.into_iter().chain(field_names).zip(fields).map(
                        |(name, field)| {
                            quote! {
                                #dict_ident.insert_value(#name, &#field);
                            }
                        },
                    );
                    quote! {
                        let #dict_ident = #glib::VariantDict::new(::std::option::Option::None);
                        #(#fields)*
                        unsafe { #dict_ident.end_unsafe() }
                    }
                } else {
                    quote! { #glib::Variant::tuple_from_iter([#(#fields),*]) }
                };

            if final_ {
                let head = def.trait_head(&wrapper_ty, quote! { #glib::ToVariant });
                quote! {
                    #head {
                        fn to_variant(&#self_ident) -> #glib::Variant {
                            #create_variant
                        }
                    }
                }
            } else {
                let to_variant = if !child_types.is_empty() {
                    let head = def.trait_head(&wrapper_ty, quote! { #glib::ToVariant });
                    let casts = serialize_child_types(
                        &*child_types,
                        &wrapper_ty,
                        def.base == TypeBase::Class && !abstract_,
                        go,
                    );
                    Some(quote! {
                        #head {
                            fn to_variant(&#self_ident) -> #glib::Variant {
                                #casts
                            }
                        }
                    })
                } else if !abstract_ {
                    let head = def.trait_head(&wrapper_ty, quote! { #glib::ToVariant });
                    Some(quote! {
                        #head {
                            #[inline]
                            fn to_variant(&#self_ident) -> #glib::Variant {
                                #go::ToParentVariant::to_parent_variant(#self_ident)
                            }
                        }
                    })
                } else {
                    None
                };
                let head = def.trait_head(&wrapper_ty, quote! { #go::ToParentVariant });
                quote! {
                    #to_variant
                    #[doc(hidden)]
                    #head {
                        fn to_parent_variant(&#self_ident) -> #glib::Variant {
                            #create_variant
                        }
                    }
                }
            }
        });

    let from_variant = from.then(|| {
        let obj_ident = syn::Ident::new("obj", Span::mixed_site());
        let args_ident = syn::Ident::new("args", Span::mixed_site());
        let variant_ident = syn::Ident::new("variant", Span::mixed_site());
        let value_ident = syn::Ident::new("value", Span::mixed_site());
        let name_ident = syn::Ident::new("name", Span::mixed_site());
        let parent_ty = (!skip_parent).then(|| parent_type).flatten();

        let construct_parent = parent_ty.map(|pty| {
            let parent_name = parent_name.as_ref().map(|n| n.to_string());
            if attr.dict.is_some() {
                quote! {
                    if #name_ident == #parent_name {
                        <#pty as #go::FromParentVariant>::push_parent_values(
                            &#variant_ident,
                            &mut #args_ident
                        );
                        continue;
                    }
                }
            } else {
                quote! {
                    <#pty as #go::FromParentVariant>::push_parent_values(
                        &#variant_ident.try_child_value(0usize)?,
                        &mut #args_ident
                    );
                }
            }
        }).into_iter();
        let start = parent_ty.map(|_| 1).unwrap_or(0);
        let construct_args = props.iter().enumerate().filter_map(|(index, (attrs, prop))| {
            if !prop.set.is_allowed() || attrs.skip_from.is_some() {
                return None;
            }
            let name = prop.name.to_string();
            let ty = prop.store_write_type(go);
            let convert = if let Some(with) = &attrs.with {
                quote_spanned! { with.span() => #with::from_variant }
            } else if let Some(from_variant) = &attrs.from {
                quote_spanned! { from_variant.span() => #from_variant }
            } else {
                quote! { #glib::FromVariant::from_variant }
            };
            Some(if attr.dict.is_some() {
                quote! {
                    if #name_ident == #name {
                        let #value_ident: #ty = #convert(&#variant_ident)?;
                        #args_ident.push((#name, #go::glib::ToValue::to_value(&#value_ident)));
                        continue;
                    }
                }
            } else {
                let index = index + start;
                quote! {
                    let #value_ident: #ty = #convert(&#variant_ident.try_child_value(#index)?)?;
                    #args_ident.push((#name, #go::glib::ToValue::to_value(&#value_ident)));
                }
            })
        });
        let construct_args = construct_parent.chain(construct_args).collect::<Vec<_>>();
        let construct_args = if attr.dict.is_some() {
            quote! {
                for #variant_ident in #variant_ident.iter() {
                    let #name_ident = #variant_ident.child_value(0);
                    let #name_ident = #name_ident.str()?;
                    let #variant_ident = #variant_ident.child_value(1).as_variant()?;
                    #(#construct_args)*
                }
            }
        } else {
            quote! { #(#construct_args)* }
        };

        let variant_ty = if attr.dict.is_some() {
            quote! { #go::glib::VariantTy::VARDICT }
        } else {
            quote! { #go::glib::VariantTy::TUPLE }
        };

        let push_current = final_.then(|| {
            quote! { #construct_args }
        }).unwrap_or_else(|| quote! {
            <Self as #go::FromParentVariant>::push_parent_values(
                &#variant_ident,
                &mut #args_ident
            );
        });
        let construct_obj = construct_obj_call(def, go, errors);
        let construct_obj = quote! {
            let mut #args_ident = ::std::vec::Vec::new();
            if !#variant_ident.is_type(#variant_ty) {
                return ::std::option::Option::None;
            }
            #push_current
            let #obj_ident = #construct_obj.ok()?;
            #glib::Cast::downcast(#obj_ident).ok()
        };
        if final_ {
            let head = def.trait_head(&wrapper_ty, quote! { #glib::FromVariant });
            quote! {
                #head {
                    fn from_variant(#variant_ident: &#glib::Variant) -> ::std::option::Option<Self> {
                        #construct_obj
                    }
                }
            }
        } else {
            let from_variant = if !child_types.is_empty() {
                let head = def.trait_head(&wrapper_ty, quote! { #glib::FromVariant });
                let casts = deserialize_child_types(
                    &*child_types,
                    &wrapper_ty,
                    def.base == TypeBase::Class && !abstract_,
                    go,
                );
                Some(quote! {
                    #head {
                        fn from_variant(#variant_ident: &#glib::Variant) -> ::std::option::Option<Self> {
                            #casts
                        }
                    }
                })
            } else if !abstract_ {
                let head = def.trait_head(&wrapper_ty, quote! { #glib::FromVariant });
                Some(quote! {
                    #head {
                        #[inline]
                        fn from_variant(#variant_ident: &#glib::Variant) -> ::std::option::Option<Self> {
                            #go::FromParentVariant::from_parent_variant(#variant_ident)
                        }
                    }
                })
            } else {
                None
            };
            let head = def.trait_head(&wrapper_ty, quote! { #go::FromParentVariant });
            quote! {
                #from_variant
                #head {
                    fn from_parent_variant(#variant_ident: &#glib::Variant) -> ::std::option::Option<Self> {
                        #construct_obj
                    }
                    fn push_parent_values(
                        #variant_ident: &#glib::Variant,
                        #args_ident: &mut ::std::vec::Vec<(&'static ::std::primitive::str, #glib::Value)>
                    ) {
                        if !#variant_ident.is_type(#variant_ty) {
                            return;
                        }
                        (|| {
                            #construct_args
                            ::std::option::Option::Some(())
                        })();
                    }
                }
            }
        }
    });

    def.ensure_items().push(syn::Item::Verbatim(quote! {
        #static_variant_type
        #to_variant
        #from_variant
    }));
}

#[inline]
fn construct_obj_call(
    _def: &TypeDefinition,
    go: &syn::Path,
    _errors: &util::Errors,
) -> TokenStream {
    let args_ident = syn::Ident::new("args", Span::mixed_site());
    #[cfg(feature = "gio")]
    {
        use gobject_core::TypeMode;
        if _def.has_method(TypeMode::Subclass, "init") {
            return quote! {
                #go::gio::Initable::with_values(
                    <Self as #go::glib::StaticType>::static_type(),
                    &#args_ident,
                )
            };
        }
        if let Some(method) = _def.find_method(TypeMode::Subclass, "init_future") {
            _errors.push_spanned(
                method,
                "AsyncInitable objects without Initable cannot be converted from variant, implement a blocking constructor with `init`"
            );
        }
    }

    quote! {
        #go::glib::Object::with_values(
            <Self as #go::glib::StaticType>::static_type(),
            &#args_ident,
        )
    }
}

#[inline]
fn has_name(attr: &syn::Attribute, name: &str, is_named: &mut bool) -> bool {
    if !attr.path.is_ident("serde") {
        return false;
    }
    attr.parse_meta()
        .map(|m| match m {
            syn::Meta::List(l) => l.nested.iter().any(|m| match m {
                syn::NestedMeta::Meta(syn::Meta::NameValue(m)) => {
                    if m.path.is_ident("rename") || m.path.is_ident("alias") {
                        if m.path.is_ident("rename") {
                            *is_named = true;
                        }
                        if let syn::Lit::Str(s) = &m.lit {
                            if s.value() == name {
                                return true;
                            }
                        }
                    }
                    false
                }
                _ => false,
            }),
            _ => false,
        })
        .unwrap_or(false)
}

#[inline]
fn serialize_child_types(
    child_types: &[syn::Path],
    wrapper_ty: &syn::Path,
    fallback: bool,
    go: &syn::Path,
) -> TokenStream {
    let self_ident = syn::Ident::new("self", Span::mixed_site());
    let obj_ident = syn::Ident::new("obj", Span::mixed_site());
    let child_casts = child_types.iter().map(|child_ty| {
        quote! {
            if let Some(#obj_ident) = #go::glib::Cast::downcast_ref::<#child_ty>(#self_ident) {
                return #go::glib::ToVariant::to_variant(&(
                    <#child_ty as #go::glib::StaticType>::static_type().name(),
                    #go::glib::ToVariant::to_variant(#obj_ident),
                ));
            }
        }
    });
    let fallback = fallback
        .then(|| {
            quote! {
                #go::glib::ToVariant::to_variant(&(
                    <#wrapper_ty as #go::glib::StaticType>::static_type().name(),
                    <#wrapper_ty as #go::ToParentVariant>::to_parent_variant(#self_ident)
                ))
            }
        })
        .unwrap_or_else(|| {
            quote! {
                ::std::panic!(
                    "Unsupported ToVariant type `{}`",
                    #go::glib::prelude::ObjectExt::type_(#self_ident).name(),
                )
            }
        });
    quote! {
        #(#child_casts)*
        #fallback
    }
}

#[inline]
fn deserialize_child_types(
    child_types: &[syn::Path],
    wrapper_ty: &syn::Path,
    fallback: bool,
    go: &syn::Path,
) -> TokenStream {
    let name_ident = syn::Ident::new("name", Span::mixed_site());
    let variant_ident = syn::Ident::new("variant", Span::mixed_site());
    let mappings = child_types.iter().map(|cty| {
        quote! {
            if #name_ident == <#cty as #go::glib::StaticType>::static_type().name() {
                let #variant_ident = <#cty as #go::glib::FromVariant>::from_variant(&#variant_ident)?;
                return ::std::option::Option::Some(#go::glib::Cast::upcast(#variant_ident));
            }
        }
    });
    let fallback_mapping = fallback.then(|| {
        quote! {
            if #name_ident == <#wrapper_ty as #go::glib::StaticType>::static_type().name() {
                let #variant_ident = <#wrapper_ty as #go::FromParentVariant>::from_parent_variant(&#variant_ident)?;
                return ::std::option::Option::Some(#go::glib::Cast::upcast(#variant_ident));
            }
        }
    });
    quote! {
        if !#variant_ident.is_type(#go::glib::VariantTy::TUPLE) || #variant_ident.n_children() != 2 {
            return ::std::option::Option::None;
        }
        let #name_ident = #variant_ident.child_value(0);
        if !#name_ident.is_type(#go::glib::VariantTy::STRING) {
            return ::std::option::Option::None;
        }
        let #name_ident = #name_ident.str()?;
        let #variant_ident = #variant_ident.child_value(1).as_variant()?;
        #(#mappings)*
        #fallback_mapping
        ::std::option::Option::None
    }
}

#[derive(Debug, Default, FromMeta)]
#[darling(default)]
struct EnumAttrs {
    pub to: Flag,
    pub from: Flag,
    pub parent: Option<syn::Path>,
    pub fallback: Flag,
    pub child_types: PathList,
}

pub(crate) fn downcast_enum(
    args: TokenStream,
    go: &syn::Path,
    errors: &util::Errors,
) -> TokenStream {
    let span = args.span();
    let attrs = util::parse_list::<EnumAttrs>(args, errors);
    let ty = match &attrs.parent {
        Some(ty) => ty,
        None => return Default::default(),
    };
    if attrs.parent.is_none() {
        errors.push(span, "Missing required attribute `parent`");
    }
    if attrs.to.is_none() && attrs.from.is_none() {
        errors.push(
            span,
            "Must have at least one of these attributes: `to`, `from`",
        );
    }
    let to_variant = attrs.to.is_some().then(|| {
        let casts = serialize_child_types(&*attrs.child_types, ty, attrs.fallback.is_some(), go);
        let obj_ident = syn::Ident::new("obj", Span::mixed_site());
        quote! {
            fn to_variant(#obj_ident: &#ty) -> #go::glib::Variant {
                #casts
            }
        }
    });
    let from_variant = attrs.from.is_some().then(|| {
        let casts = deserialize_child_types(&*attrs.child_types, ty, attrs.fallback.is_some(), go);
        let variant_ident = syn::Ident::new("variant", Span::mixed_site());
        quote! {
            fn from_variant(#variant_ident: &#go::glib::Variant) -> ::std::option::Option<#ty> {
                #casts
            }
        }
    });
    quote! {
        #to_variant
        #from_variant
    }
}
