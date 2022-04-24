use super::actions::Action;
use darling::{util::SpannedValue, FromAttributes};
use gobject_core::{
    util::{self, Errors},
    ClassDefinition, PropertyStorage, TypeContext, TypeMode,
};
use heck::ToKebabCase;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use std::collections::HashMap;
use syn::{parse_quote, parse_quote_spanned, spanned::Spanned};

#[derive(Default, FromAttributes)]
#[darling(default, attributes(widget_actions))]
struct WidgetActionAttrs {
    group: Option<String>,
    insert: SpannedValue<Option<bool>>,
    bind: Option<syn::Expr>,
}

#[derive(Default, FromAttributes)]
#[darling(default, attributes(action_group))]
struct ActionGroupAttrs {
    name: Option<String>,
}

#[derive(Default, FromAttributes)]
#[darling(default, attributes(action))]
struct PropertyActionAttrs {
    group: Option<String>,
    name: Option<String>,
}

struct PropertyAction {
    name: String,
    property: String,
}

impl PropertyAction {
    #[inline]
    fn to_token_stream(&self, this_ident: &syn::Ident, go: &syn::Ident) -> TokenStream {
        let name = &self.name;
        let property = &self.property;
        quote! {
            #go::gio::PropertyAction::new(#name, #this_ident, #property)
        }
    }
}

#[derive(Default)]
struct WidgetActionGroup {
    actions: Vec<Action>,
    properties: Vec<PropertyAction>,
    no_insert: bool,
    bind: Option<syn::Expr>,
}

impl WidgetActionGroup {
    fn is_empty(&self) -> bool {
        self.actions.is_empty() && self.properties.is_empty()
    }
    fn to_tokens(
        &self,
        name: &str,
        this_ident: &syn::Ident,
        go: &syn::Ident,
    ) -> Option<TokenStream> {
        if self.is_empty() {
            return None;
        }
        let group_ident = syn::Ident::new("group", Span::mixed_site());
        let actions = self.actions.iter().map(|action| {
            let action = action.to_token_stream(this_ident, true, go);
            quote! { #go::gio::prelude::ActionMapExt::add_action(&#group_ident, &#action); }
        });
        let properties = self.properties.iter().map(|action| {
            let action = action.to_token_stream(this_ident, go);
            quote! { #go::gio::prelude::ActionMapExt::add_action(&#group_ident, &#action); }
        });

        let insert = (!self.no_insert).then(|| {
            quote! {
                #go::gtk4::prelude::WidgetExt::insert_action_group(
                    #this_ident,
                    #name,
                    ::std::option::Option::Some(&#group_ident),
                );
            }
        });
        let bind = self.bind.as_ref().map(|expr| {
            quote_spanned! { expr.span() =>
                #go::ParamStoreWrite::set_owned(
                    &#go::glib::subclass::prelude::ObjectSubclassIsExt::imp(#this_ident).#expr,
                    #group_ident
                );
            }
        });
        Some(quote! {
            {
                let #group_ident = #go::gio::SimpleActionGroup::new();
                #(#actions)*
                #(#properties)*
                #insert
                #bind
            }
        })
    }
}

pub(crate) fn extend_widget_actions(def: &mut ClassDefinition, errors: &Errors) {
    let mut groups = HashMap::<String, WidgetActionGroup>::new();
    let default_group = match def.inner.name.as_ref() {
        Some(n) => n.to_string().to_kebab_case(),
        None => return,
    };
    let wrapper_ty = def
        .inner
        .type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
    let wrapper_ty = match wrapper_ty {
        Some(ty) => ty,
        None => return,
    };
    for impl_ in def.inner.methods_items_mut() {
        if let Some(mode) = TypeMode::for_item_type(&*impl_.self_ty) {
            let attrs = util::extract_attrs(&mut impl_.attrs, "widget_actions")
                .map(|a| util::parse_attributes::<WidgetActionAttrs>(&a, errors))
                .unwrap_or_default();
            let group = attrs.group.unwrap_or_else(|| default_group.clone());
            let mut group = groups.entry(group).or_default();
            if !attrs.insert.unwrap_or(true) {
                if group.no_insert {
                    errors.push(attrs.insert.span(), "Duplicate `insert = false` attribute");
                }
                group.no_insert = true;
            }
            if let Some(bind) = attrs.bind {
                if group.bind.is_some() {
                    errors.push_spanned(&bind, "Duplicate `bind` attribute");
                }
                group.bind = Some(bind);
            }
            Action::many_from_items(&mut impl_.items, &mut group.actions, mode, errors);
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
                if let Some(attrs) = util::extract_attrs(&mut f.attrs, "action") {
                    let attr = util::parse_attributes::<PropertyActionAttrs>(&attrs, errors);
                    let group = attr.group.unwrap_or_else(|| default_group.clone());
                    let name = attr.name.unwrap_or_else(|| property.clone());
                    let group = groups.entry(group).or_default();
                    if group.actions.iter().any(|a| a.name == name) {
                        errors.push_spanned(f, format!("Duplicate `{}` action", name));
                    }
                    group.properties.push(PropertyAction {
                        name,
                        property: property.clone(),
                    });
                }
            }
        }
        for (index, field) in item.fields.iter_mut().enumerate() {
            (|| {
                let attrs = util::extract_attrs(&mut field.attrs, "action_group")?;
                let attr = util::parse_attributes::<ActionGroupAttrs>(&attrs, errors);
                let name = attr.name
                    .or_else(|| field.ident.as_ref().map(|i| i.to_string().to_kebab_case()))
                    .or_else(|| {
                        errors.push_spanned(
                            attrs.first().unwrap(),
                            "action group field must be named or have #[action_group(name = \"...\")]"
                        );
                        None
                    })?;
                let group = groups.get_mut(&name).or_else(|| {
                    errors.push_spanned(
                        attrs.first().unwrap(),
                        format!("No actions defined for group `{}`", name),
                    );
                    None
                })?;
                if let Some(bind) = &group.bind {
                    errors.push_spanned(
                        bind,
                        format!(
                            "`bind` attribute for `{}` already specified on #[action_group]",
                            name
                        ),
                    );
                }
                group.bind = Some(
                    field
                        .ident
                        .as_ref()
                        .map(|i| parse_quote_spanned! { i.span() => #i })
                        .unwrap_or_else(|| parse_quote_spanned! { field.span() => #index }),
                );
                None::<()>
            })();
        }
    }
    if groups.values().all(|g| g.is_empty()) {
        return;
    }
    let go = def.inner.crate_ident.clone();
    for (group_name, group) in &groups {
        if group.is_empty() {
            if let Some(bind) = &group.bind {
                errors.push_spanned(
                    bind,
                    format!("No actions defined for group `{}`", group_name),
                );
            }
        }
        super::actions::validate_actions(&group.actions, errors);
        for action in &group.actions {
            if let Some(bind) = &group.bind {
                action.override_public_methods(Some(bind), def, errors);
            } else {
                if let Some(handler) = &action.activate {
                    if let Some(pm) = def
                        .inner
                        .public_method_mut(handler.mode, &handler.sig.ident)
                    {
                        if let Some((custom_tag, _)) = pm.custom_body.as_ref() {
                            errors.push(
                                pm.sig.span(),
                                format!(
                                    "#[action] cannot be used on public method already overriden by {}",
                                    custom_tag
                                ),
                            );
                        }
                        action.prepare_public_method(handler, pm, def.final_, errors);
                        let self_ident = syn::Ident::new("self", Span::mixed_site());
                        pm.sig.inputs[0] =
                            parse_quote_spanned! { Span::mixed_site() => &#self_ident };
                        let param = handler
                            .parameter_index
                            .map(|(_, span)| {
                                let glib = quote! { #go::glib };
                                let param_ident = syn::Ident::new("param", Span::mixed_site());
                                let param = handler
                                    .parameter_to(action, &glib)
                                    .map(|path| quote_spanned! { path.span() => #path(&#param_ident) })
                                    .unwrap_or_else(|| quote! { #param_ident });
                                quote_spanned! { span => ::std::option::Option::Some(&#param) }
                            })
                            .unwrap_or_else(|| quote! { ::std::option::Option::None });
                        let name = format!("{}.{}", group_name, action.name);
                        pm.custom_body = Some((
                            String::from("#[action]"),
                            Box::new(parse_quote_spanned! { handler.sig.span() => {
                                #go::gtk4::prelude::WidgetExt::activate_action(
                                    #go::glib::Cast::upcast_ref::<#wrapper_ty>(#self_ident),
                                    #name,
                                    #param,
                                ).unwrap();
                            }}),
                        ));
                    }
                }
                if let Some(handler) = &action.change_state {
                    if def
                        .inner
                        .public_method(handler.mode, &handler.sig.ident)
                        .is_some()
                    {
                        errors.push(
                            handler.span(),
                            "Public widget change-state handler must have `bind = \"...\"` on the group, or an #[action_group] field on the struct",
                        );
                    }
                }
            }
        }
    }
    let this_ident = syn::Ident::new("obj", Span::mixed_site());
    let groups = groups
        .iter()
        .filter_map(|(name, group)| group.to_tokens(name, &this_ident, &go));
    def.inner.add_custom_stmt(
        "instance_init",
        parse_quote! {
            {
                let #this_ident = unsafe { #this_ident.as_ref() };
                #(#groups)*
            };
        },
    );
}
