use crate::actions::ParameterType;
use darling::{
    util::{Flag, SpannedValue},
    FromAttributes,
};
use gobject_core::{
    util::{self, Errors},
    ClassDefinition, PropertyStorage, PublicMethod, TypeContext, TypeMode,
};
use heck::ToKebabCase;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use std::{borrow::Cow, collections::HashSet};
use syn::{parse_quote, parse_quote_spanned, spanned::Spanned};

#[derive(Default, FromAttributes)]
#[darling(default, attributes(widget_action))]
struct WidgetActionAttrs {
    name: Option<syn::LitStr>,
    group: Option<syn::LitStr>,
    #[darling(rename = "type")]
    type_: Option<SpannedValue<ParameterType>>,
    type_str: Option<syn::LitStr>,
    disabled: SpannedValue<Flag>,
    with: Option<syn::Path>,
    to: Option<syn::Path>,
    from: Option<syn::Path>,
}

impl WidgetActionAttrs {
    #[inline]
    fn validate(&self, errors: &Errors) {
        use gobject_core::validations;

        let type_ = ("type", validations::check_spanned(&self.type_));
        let type_str = ("type_str", validations::check_spanned(&self.type_str));
        let with = ("with", validations::check_spanned(&self.with));
        let to = ("to", validations::check_spanned(&self.to));
        let from = ("from", validations::check_spanned(&self.from));

        validations::only_one([&type_, &type_str], errors);
        validations::only_one([&with, &to], errors);
        validations::only_one([&with, &from], errors);
        validations::only_one([&type_str, &with], errors);
        validations::only_one([&type_str, &to], errors);
        validations::only_one([&type_str, &from], errors);
    }
}

#[derive(Default, FromAttributes)]
#[darling(default, attributes(widget_action))]
struct PropertyActionAttrs {
    name: Option<syn::LitStr>,
    group: Option<syn::LitStr>,
    disabled: SpannedValue<Flag>,
}

#[inline]
fn make_name(
    ty_ident: &syn::Ident,
    action_ident: impl std::fmt::Display,
    name: Option<syn::LitStr>,
    group: Option<syn::LitStr>,
) -> String {
    let group = group
        .map(|g| g.value())
        .unwrap_or_else(|| ty_ident.to_string().to_kebab_case());
    let name = name
        .map(|g| g.value())
        .unwrap_or_else(|| action_ident.to_string().to_kebab_case());
    format!("{}.{}", group, name)
}

struct WidgetAction {
    name: String,
    sig: syn::Signature,
    mode: TypeMode,
    parameter_type: ParameterType,
    parameter_index: Option<(usize, Span)>,
    disabled: bool,
    parameter_to: Option<syn::Path>,
    parameter_from: Option<syn::Path>,
}

impl WidgetAction {
    #[inline]
    fn from_method(
        method: &mut syn::ImplItemMethod,
        attr: WidgetActionAttrs,
        ty_ident: &syn::Ident,
        mode: TypeMode,
        errors: &Errors,
    ) -> Self {
        if let syn::ReturnType::Type(_, ty) = &method.sig.output {
            errors.push_spanned(ty, "Widget action cannot have return type");
        }
        let skip = if let Some(recv) = method.sig.receiver() {
            if mode == TypeMode::Subclass && util::arg_reference(recv).is_none() {
                errors.push_spanned(recv, "Subclass action receiver must be `&self`");
            }
            1
        } else {
            0
        };
        let mut parameter_index = None;
        for (index, arg) in method.sig.inputs.iter_mut().enumerate().skip(skip) {
            let arg = match arg {
                syn::FnArg::Typed(t) => t,
                _ => continue,
            };
            if parameter_index.is_some() {
                errors.push_spanned(arg, "Duplicate parameter argument");
            } else {
                parameter_index = Some((index, arg.span()));
            }
        }
        let parameter_type = attr
            .type_
            .map(|t| (*t).clone())
            .or_else(|| attr.type_str.map(ParameterType::String))
            .or_else(|| {
                attr.with.as_ref().map(|path| {
                    ParameterType::Path(parse_quote_spanned! { path.span() =>
                        #path::static_variant_type
                    })
                })
            })
            .unwrap_or_default();
        let parameter_to = attr.to.or_else(|| {
            let path = attr.with.as_ref()?;
            Some(parse_quote_spanned! { path.span() => #path::to_variant })
        });
        let parameter_from = attr.from.or_else(|| {
            let path = attr.with.as_ref()?;
            Some(parse_quote_spanned! { path.span() => #path::from_variant })
        });
        let name = make_name(ty_ident, &method.sig.ident, attr.name, attr.group);
        Self {
            name,
            sig: method.sig.clone(),
            mode,
            parameter_type,
            parameter_index,
            disabled: attr.disabled.is_some(),
            parameter_to,
            parameter_from,
        }
    }
    fn prepare_public_method(
        &self,
        public_method: &mut PublicMethod,
        final_: bool,
        errors: &Errors,
    ) {
        if self.mode == TypeMode::Wrapper
            && !matches!(
                &public_method.constructor,
                Some(gobject_core::ConstructorType::Auto { .. })
            )
            && public_method.target.is_none()
            && final_
        {
            errors.push(
                self.sig.span(),
                "widget action using #[public] on wrapper type for final class must be renamed with #[public(name = \"...\")]",
            );
        }
        public_method.sig.asyncness = None;
        public_method.sig.inputs = self
            .sig
            .inputs
            .iter()
            .cloned()
            .enumerate()
            .map(|(i, mut arg)| {
                if Some(i) == self.parameter_index.map(|p| p.0) {
                    if let syn::FnArg::Typed(ty) = &mut arg {
                        ty.pat = parse_quote_spanned! { Span::mixed_site() => param };
                    }
                }
                arg
            })
            .collect();
        if let Some(recv) = self.sig.receiver() {
            let mut recv = recv.clone();
            match &mut recv {
                syn::FnArg::Receiver(recv) => {
                    recv.self_token = parse_quote_spanned! { Span::mixed_site() => self }
                }
                syn::FnArg::Typed(pat) => {
                    if let syn::Pat::Ident(p) = &mut *pat.pat {
                        p.ident = parse_quote_spanned! { Span::mixed_site() => self };
                    }
                }
            }
            public_method.sig.inputs[0] = recv;
        } else {
            public_method
                .sig
                .inputs
                .insert(0, parse_quote_spanned! { Span::mixed_site() => &self });
        }
        if public_method.constructor.is_some() {
            errors.push(
                public_method.sig.span(),
                "#[action] cannot be used on constructor",
            );
        } else if let Some((custom_tag, _)) = public_method.custom_body.as_ref() {
            errors.push(
                public_method.sig.span(),
                format!(
                    "#[action] cannot be used on public method already overriden by {}",
                    custom_tag
                ),
            );
        }
    }
    #[inline]
    fn parameter_convert_type(&self) -> Option<&syn::Type> {
        if !self.parameter_type.needs_convert() {
            return None;
        }
        self.parameter_type()
    }
    fn parameter_type(&self) -> Option<&syn::Type> {
        self.parameter_index.and_then(|(index, _)| {
            let param = self.sig.inputs.iter().nth(index);
            match param {
                Some(syn::FnArg::Typed(p)) => Some(&*p.ty),
                _ => None,
            }
        })
    }
    fn parameter_from<'a>(&'a self, glib: &syn::Path) -> Option<Cow<'a, syn::Path>> {
        if let Some(from) = &self.parameter_from {
            Some(Cow::Borrowed(from))
        } else if self.parameter_type.needs_convert() {
            Some(Cow::Owned(
                parse_quote! { #glib::FromVariant::from_variant },
            ))
        } else {
            None
        }
    }
    fn parameter_to<'a>(&'a self, glib: &syn::Path) -> Option<Cow<'a, syn::Path>> {
        if let Some(to) = &self.parameter_to {
            Some(Cow::Borrowed(to))
        } else if self.parameter_type.needs_convert() {
            Some(Cow::Owned(parse_quote! { #glib::ToVariant::to_variant }))
        } else {
            None
        }
    }
    fn to_token_stream(&self, class_ident: &syn::Ident, go: &syn::Path) -> TokenStream {
        let name = &self.name;
        let glib: syn::Path = parse_quote! { #go::glib };
        let parameter_type = self
            .parameter_type
            .type_expr(self.parameter_convert_type(), &glib);
        let type_option = parameter_type
            .as_ref()
            .map(|ty| quote! { ::std::option::Option::Some(#ty.as_str()) })
            .unwrap_or_else(|| quote! { ::std::option::Option::None });
        let this_ident = syn::Ident::new("_obj", Span::mixed_site());
        let param_ident = syn::Ident::new("_param", Span::mixed_site());
        let self_ty = match self.mode {
            TypeMode::Wrapper => quote! {
                <Self as #go::glib::subclass::types::ObjectSubclass>::Type
            },
            _ => quote! { Self },
        };
        let before = [
            self.sig.receiver().and_then(|recv| match self.mode {
                TypeMode::Subclass => {
                    Some(quote_spanned! { recv.span() =>
                        let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#this_ident);
                    })
                },
                TypeMode::Wrapper => (
                    util::arg_reference(recv).is_none() && self.sig.asyncness.is_none()
                    ).then(|| {
                    quote_spanned! { recv.span() =>
                        let #this_ident = ::std::clone::Clone::clone(#this_ident);
                    }
                }),
            }),
            (!matches!(&self.parameter_type, ParameterType::Empty))
                .then(|| ())
                .and(self.parameter_index)
                .map(|(_, span)| {
                    let convert = self.parameter_from(&glib)
                        .map(|path| quote_spanned! { path.span() =>
                            let #param_ident = #path(#param_ident)
                                .expect("Invalid type passed for action parameter");
                            });
                    let cast_ty = self
                        .parameter_convert_type()
                        .map(|param_ty| quote_spanned! { span =>
                            let #param_ident: #param_ty = #param_ident;
                        });
                    quote_spanned! { span =>
                        let #param_ident = #param_ident.unwrap();
                        #convert
                        #cast_ty
                    }
                }),
        ].into_iter().flatten();
        let arg_names = self
            .sig
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(index, arg)| {
                if Some(arg) == self.sig.receiver() {
                    return Some(&this_ident);
                } else if Some(index) == self.parameter_index.map(|i| i.0) {
                    return Some(&param_ident);
                }
                None
            });
        let ident = &self.sig.ident;
        let call = quote_spanned! { self.sig.span() =>
            #(#before)*
            #self_ty::#ident(#(#arg_names),*)
        };
        let call = self
            .sig
            .asyncness
            .as_ref()
            .map(|_| {
                let recv = self.sig.receiver();
                let outer = [
                    recv.map(|_| {
                        quote! {
                            let #this_ident = ::std::clone::Clone::clone(#this_ident);
                        }
                    }),
                    self.parameter_index.map(|_| {
                        quote! {
                            let #param_ident = #param_ident.cloned();
                        }
                    }),
                ]
                .into_iter()
                .flatten();
                let inner = [
                    (self.mode == TypeMode::Subclass
                        || recv.and_then(util::arg_reference).is_some())
                    .then(|| {
                        quote! {
                            let #this_ident = &#this_ident;
                        }
                    }),
                    self.parameter_index.map(|_| {
                        quote! {
                            let #param_ident = #param_ident.as_ref();
                        }
                    }),
                ]
                .into_iter()
                .flatten();
                quote! {
                    #(#outer)*
                    #glib::MainContext::default().spawn_local(async move {
                        #(#inner)*
                        #call.await;
                    });
                }
            })
            .unwrap_or_else(|| quote! { #call; });
        quote! {
            #go::gtk4::subclass::widget::WidgetClassSubclassExt::install_action(
                #class_ident,
                #name,
                #type_option,
                |#this_ident, _, #param_ident| {
                    #call
                },
            );
        }
    }
}

struct PropertyAction {
    name: String,
    property: String,
    disabled: bool,
}

impl PropertyAction {
    #[inline]
    fn to_token_stream(&self, class_ident: &syn::Ident, go: &syn::Path) -> TokenStream {
        let name = &self.name;
        let property = &self.property;
        quote! {
            #go::gtk4::subclass::widget::WidgetClassSubclassExt::install_property_action(
                #class_ident,
                #name,
                #property,
            );
        }
    }
}

pub(crate) fn extend_widget_actions(def: &mut ClassDefinition, errors: &Errors) {
    let mut names = HashSet::new();
    let mut actions = Vec::new();
    let mut property_actions = Vec::new();
    let wrapper_ty = def
        .inner
        .type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
    let ty_ident = def.inner.name.clone();
    for impl_ in def.inner.methods_items_mut() {
        if let Some(mode) = TypeMode::for_item_type(&*impl_.self_ty) {
            for item in &mut impl_.items {
                if let syn::ImplItem::Method(method) = item {
                    if let Some(attrs) = util::extract_attrs(&mut method.attrs, "widget_action") {
                        let attr = util::parse_attributes::<WidgetActionAttrs>(&attrs, errors);
                        attr.validate(errors);
                        let action =
                            WidgetAction::from_method(method, attr, &ty_ident, mode, errors);
                        if names.contains(&action.name) {
                            errors.push_spanned(
                                &action.sig,
                                format!("Duplicate action `{}`", action.name),
                            );
                        } else {
                            names.insert(action.name.clone());
                        }
                        actions.push(action);
                    }
                }
            }
        }
    }
    let properties = def
        .inner
        .properties
        .iter()
        .map(|p| (p.name.to_string(), p.storage.clone()))
        .collect::<Vec<_>>();
    if let Some(item) = def.inner.properties_item_mut() {
        for (property, storage) in &properties {
            let field = match &storage {
                PropertyStorage::NamedField(ident) => item
                    .fields
                    .iter_mut()
                    .find(|f| f.ident.as_ref() == Some(ident)),
                PropertyStorage::UnnamedField(id) => item.fields.iter_mut().nth(*id),
                _ => None,
            };
            if let Some(f) = field {
                if let Some(attrs) = util::extract_attrs(&mut f.attrs, "widget_action") {
                    let attr = util::parse_attributes::<PropertyActionAttrs>(&attrs, errors);
                    let name = make_name(&ty_ident, property, attr.name, attr.group);
                    let action = PropertyAction {
                        name,
                        property: property.clone(),
                        disabled: attr.disabled.is_some(),
                    };
                    if names.contains(&action.name) {
                        errors.push_spanned(f, format!("Duplicate action `{}`", action.name));
                    } else {
                        names.insert(action.name.clone());
                    }
                    property_actions.push(action);
                }
            }
        }
    }
    if actions.is_empty() && property_actions.is_empty() {
        return;
    }
    let go = def.inner.crate_path.clone();
    for action in &actions {
        if let Some(pm) = def.inner.public_method_mut(action.mode, &action.sig.ident) {
            if let Some((custom_tag, _)) = pm.custom_body.as_ref() {
                errors.push(
                    pm.sig.span(),
                    format!(
                        "#[action] cannot be used on public method already overriden by {}",
                        custom_tag
                    ),
                );
            }
            action.prepare_public_method(pm, def.final_, errors);
            let self_ident = syn::Ident::new("self", Span::mixed_site());
            pm.sig.inputs[0] = parse_quote_spanned! { Span::mixed_site() => &#self_ident };
            let param = action
                .parameter_index
                .map(|(_, span)| {
                    let glib: syn::Path = parse_quote! { #go::glib };
                    let param_ident = syn::Ident::new("param", Span::mixed_site());
                    let param = action
                        .parameter_to(&glib)
                        .map(|path| quote_spanned! { path.span() => #path(&#param_ident) })
                        .unwrap_or_else(|| quote! { #param_ident });
                    quote_spanned! { span => ::std::option::Option::Some(&#param) }
                })
                .unwrap_or_else(|| quote! { ::std::option::Option::None });
            let name = &action.name;
            pm.custom_body = Some((
                String::from("#[widget_action]"),
                Box::new(parse_quote_spanned! { action.sig.span() => {
                    #go::gtk4::prelude::WidgetExt::activate_action(
                        #go::glib::Cast::upcast_ref::<#wrapper_ty>(#self_ident),
                        #name,
                        #param,
                    ).unwrap();
                }}),
            ));
        }
    }
    {
        let class_ident = syn::Ident::new("class", Span::mixed_site());
        let actions = actions.iter().map(|a| a.to_token_stream(&class_ident, &go));
        let property_actions = property_actions
            .iter()
            .map(|a| a.to_token_stream(&class_ident, &go));
        def.inner.add_custom_stmt(
            "class_init",
            parse_quote! {
                {
                    #(#actions)*
                    #(#property_actions)*
                };
            },
        );
    }
    let mut disables = actions
        .iter()
        .filter_map(|a| a.disabled.then(|| &a.name))
        .chain(
            property_actions
                .iter()
                .filter_map(|a| a.disabled.then(|| &a.name)),
        )
        .peekable();
    if disables.peek().is_some() {
        let this_ident = syn::Ident::new("obj", Span::mixed_site());
        let disables = disables.map(|name| {
            quote! {
                #go::gtk4::prelude::WidgetExt::action_set_enabled(
                    #this_ident,
                    #name,
                    false,
                );
            }
        });
        def.inner.add_custom_stmt(
            "instance_init",
            parse_quote! {
                {
                    let #this_ident = unsafe { #this_ident.as_ref() };
                    #(#disables)*
                };
            },
        );
    }
}
