use darling::{
    util::{Flag, SpannedValue},
    FromMeta,
};
use gobject_core::{
    util::{self, Errors},
    validations, TypeContext, TypeMode,
};
use heck::ToKebabCase;
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use std::borrow::Cow;
use syn::{parse_quote, parse_quote_spanned, spanned::Spanned};

#[cfg(feature = "gio")]
pub(crate) fn extend_actions(def: &mut gobject_core::ClassDefinition, errors: &Errors) {
    let mut actions = Vec::new();
    for impl_ in def.inner.methods_items_mut() {
        if let Some(mode) = TypeMode::for_item_type(&*impl_.self_ty) {
            Action::many_from_items(&mut impl_.items, &mut actions, mode, errors);
        }
    }
    if actions.is_empty() {
        return;
    }
    validate_actions(&actions, errors);
    {
        use TypeContext::*;
        use TypeMode::*;
        let sub_ty = def.inner.type_(Subclass, Subclass, External);
        let sub_ty = sub_ty.as_ref();
        let wrapper_ty = def.inner.type_(Subclass, Wrapper, External);
        let wrapper_ty = wrapper_ty.as_ref();
        for a in &actions {
            try_customize_public_method(a, true, sub_ty, wrapper_ty, def, errors);
            try_customize_public_method(a, false, sub_ty, wrapper_ty, def, errors);
        }
    }
    let go = &def.inner.crate_ident;
    let this_ident = syn::Ident::new("obj", Span::mixed_site());
    let actions = actions.iter().map(|action| {
        let action = action.to_token_stream(&this_ident, go);
        quote! { #go::gio::prelude::ActionMapExt::add_action(#this_ident, &#action); }
    });
    def.inner.add_custom_stmt(
        "instance_init",
        parse_quote! {
            {
                let #this_ident = unsafe { #this_ident.as_ref() };
                #(#actions)*
            };
        },
    );
}

pub(crate) fn validate_actions(actions: &[Action], errors: &Errors) {
    let go = syn::Ident::new("go", Span::call_site());
    for action in actions {
        if let Some(change_state) = action.change_state.as_ref() {
            if action.state_type(&go).is_none() {
                errors.push(
                    change_state.span(),
                    "Action with change-state handler must have a state argument, return type, or `default` attribute"
                );
            }
        }
    }
}

fn try_customize_public_method(
    action: &Action,
    activate: bool,
    sub_ty: Option<&TokenStream>,
    wrapper_ty: Option<&TokenStream>,
    def: &mut gobject_core::ClassDefinition,
    errors: &Errors,
) -> Option<()> {
    let handler = match activate {
        true => action.activate.as_ref()?,
        false => action.change_state.as_ref()?,
    };
    let sub_ty = sub_ty?;
    let wrapper_ty = wrapper_ty?;
    let public_method = def
        .inner
        .public_methods
        .iter_mut()
        .find(|pm| pm.mode == handler.mode && pm.sig.ident == handler.sig.ident)?;
    if def.final_ && handler.mode == TypeMode::Wrapper {
        errors.push_spanned(
            &handler.sig,
            "Action on final class wrapper cannot be #[public]",
        );
        return None;
    }
    if handler.sig.receiver().is_none() {
        errors.push_spanned(&handler.sig, "Action without `self` cannot be #[public]");
        return None;
    }
    public_method.sig.output = syn::ReturnType::Default;
    public_method.sig.inputs = handler
        .sig
        .inputs
        .iter()
        .cloned()
        .enumerate()
        .filter_map(|(i, mut arg)| {
            if Some(i) == handler.parameter_index.map(|p| p.0) {
                match &mut arg {
                    syn::FnArg::Typed(ty) => {
                        ty.pat = parse_quote_spanned! { Span::mixed_site() => param };
                    }
                    _ => {}
                }
                return Some(arg);
            }
            (Some(i) != handler.state_index.map(|p| p.0)
                && Some(i) != handler.action_index.map(|p| p.0))
            .then(|| arg)
        })
        .collect();
    if let Some(recv) = handler.sig.receiver() {
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
    }
    public_method.custom_body = Some(Box::new(handler.to_public_method_expr(
        &action.name,
        sub_ty,
        wrapper_ty,
        action,
        activate,
        &def.inner.crate_ident,
    )));
    Some(())
}

#[derive(Default, FromMeta)]
#[darling(default)]
struct ActionAttrs {
    name: Option<syn::LitStr>,
    parameter_type: Option<SpannedValue<syn::LitStr>>,
    change_state: SpannedValue<Flag>,
    default: Option<SpannedValue<syn::Expr>>,
    default_variant: Option<SpannedValue<syn::Expr>>,
    hint: Option<SpannedValue<syn::Expr>>,
    disabled: SpannedValue<Flag>,
}

pub(crate) struct ActionHandler {
    pub span: Span,
    pub sig: syn::Signature,
    pub mode: TypeMode,
    pub parameter_index: Option<(usize, Span)>,
    pub state_index: Option<(usize, Span)>,
    pub action_index: Option<(usize, Span)>,
}

impl ActionHandler {
    fn new(method: &mut syn::ImplItemMethod, mode: TypeMode, errors: &Errors) -> Self {
        let skip = if let Some(recv) = method.sig.receiver() {
            if mode == TypeMode::Subclass && util::arg_reference(recv).is_none() {
                errors.push_spanned(recv, "Subclass action receiver must be `&self`");
            }
            1
        } else {
            0
        };
        let mut parameter_index = None;
        let mut state_index = None;
        let mut action_index = None;
        for (index, arg) in method.sig.inputs.iter_mut().enumerate().skip(skip) {
            let arg = match arg {
                syn::FnArg::Typed(t) => t,
                _ => continue,
            };
            if let Some(attr) = util::extract_attr(&mut arg.attrs, "state") {
                if !attr.tokens.is_empty() {
                    errors.push_spanned(&attr.tokens, "Unknown tokens on #[state]");
                }
                if state_index.is_some() {
                    errors.push_spanned(&attr, "Duplicate state argument");
                } else {
                    state_index = Some((index, arg.span()));
                }
            } else if let Some(attr) = util::extract_attr(&mut arg.attrs, "action") {
                if !attr.tokens.is_empty() {
                    errors.push_spanned(&attr.tokens, "Unknown tokens on #[action]");
                }
                if action_index.is_some() {
                    errors.push_spanned(&attr, "Duplicate action argument");
                } else {
                    action_index = Some((index, arg.span()));
                }
            } else if parameter_index.is_some() {
                errors.push_spanned(arg, "Duplicate parameter argument");
            } else {
                parameter_index = Some((index, arg.span()));
            }
        }
        let mut handler = Self {
            span: method.span(),
            sig: method.sig.clone(),
            mode,
            parameter_index,
            state_index,
            action_index,
        };
        if handler.sig.asyncness.is_some() {
            let has_return = if let Some(ret) = handler.return_type() {
                errors.push_spanned(ret, "Async action cannot have return type");
                true
            } else {
                false
            };
            if has_return {
                handler.sig.output = syn::ReturnType::Default;
            }
        }
        handler
    }
    fn parameter_type(&self) -> Option<&syn::Type> {
        self.parameter_index
            .map(|(index, _)| {
                let param = self.sig.inputs.iter().nth(index);
                match param {
                    Some(syn::FnArg::Typed(p)) => Some(&*p.ty),
                    _ => None,
                }
            })
            .flatten()
    }
    fn state_type(&self, go: &syn::Ident) -> Option<Cow<'_, syn::Type>> {
        self.state_index
            .map(|(index, _)| {
                let param = self.sig.inputs.iter().nth(index);
                match param {
                    Some(syn::FnArg::Typed(p)) => Some(Cow::Borrowed(&*p.ty)),
                    _ => None,
                }
            })
            .flatten()
            .or_else(|| {
                self.return_type().map(|ty| {
                    Cow::Owned(parse_quote_spanned! { ty.span() =>
                        <#ty as #go::ActionStateReturn>::ReturnType
                    })
                })
            })
    }
    fn return_type(&self) -> Option<&syn::Type> {
        match &self.sig.output {
            syn::ReturnType::Type(_, ty) => Some(&*ty),
            _ => None,
        }
    }
    fn to_token_stream(
        &self,
        this_ident: &syn::Ident,
        action: &Action,
        activate: bool,
        go: &syn::Ident,
    ) -> TokenStream {
        let glib = quote! { #go::glib };
        let self_ty = match self.mode {
            TypeMode::Subclass => quote! { Self },
            TypeMode::Wrapper => quote! {
                <Self as #go::glib::subclass::types::ObjectSubclass>::Type
            },
        };
        let param_ident = syn::Ident::new("param", Span::mixed_site());
        let action_ident = syn::Ident::new("action", Span::mixed_site());
        let action_in_ident = syn::Ident::new("action_in", Span::mixed_site());
        let state_ident = syn::Ident::new("state", Span::mixed_site());
        let ret_ident = syn::Ident::new("_ret", Span::mixed_site());
        let action_ref = self
            .action_index
            .and_then(|(index, _)| util::arg_reference(self.sig.inputs.iter().nth(index)?));
        let args = [
            Some(
                (self.action_index.is_some()
                    || self.state_index.is_some()
                    || self.return_type().is_some())
                .then(|| quote! { #action_ident: #action_ref #go::gio::SimpleAction })
                .unwrap_or_else(|| quote! { _ }),
            ),
            Some(
                self.parameter_index
                    .map(|_| quote! { #param_ident: #glib::Variant })
                    .unwrap_or_else(|| quote! { _ }),
            ),
            self.sig.receiver().map(|_| quote! { #[watch] #this_ident }),
        ]
        .into_iter()
        .flatten();
        let before = [
            self.sig.receiver().and_then(|recv| match self.mode {
                TypeMode::Subclass => {
                    let ref_ = self.sig.asyncness.as_ref().map(|_| quote! { & });
                    Some(quote_spanned! { recv.span() =>
                        let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ #this_ident);
                    })
                },
                TypeMode::Wrapper => util::arg_reference(recv).map(|ref_| {
                    quote_spanned! { recv.span() =>
                        let #this_ident = #ref_ #this_ident;
                    }
                }),
            }),
            (action.parameter_type.is_none())
                .then(|| ())
                .and_then(|_| self.parameter_index)
                .map(|(_, span)| {
                    let cast_ty = action.parameter_type().map(|param_ty| {
                        quote_spanned! { span =>
                            let #param_ident: #param_ty = #param_ident;
                        }
                    });
                    quote_spanned! { span =>
                        let #param_ident = #glib::FromVariant::from_variant(&#param_ident)
                            .expect("Invalid type passed for action parameter");
                        #cast_ty
                    }
                }),
            self.state_index.map(|(_, span)| {
                let unwrap = (!action.state_variant).then(|| quote_spanned! { span =>
                    let #state_ident = #glib::FromVariant::from_variant(&#state_ident)
                        .expect("Invalid state type stored in action");
                });
                let cast_ty = action.state_type(go).map(|state_ty| {
                    quote_spanned! { span =>
                        let #state_ident: #state_ty = #state_ident;
                    }
                });
                let ref_ = action_ref.is_none().then(|| quote! { & });
                quote_spanned! { span =>
                    let #state_ident = #go::gio::prelude::ActionExt::state(#ref_ #action_ident)
                        .expect("No state stored in action");
                    #unwrap
                    #cast_ty
                }
            }),
            self.action_index.map(|(_, span)| {
                let cast = action_ref
                    .as_ref()
                    .map(|_| quote! { upcast_ref(#action_ident) })
                    .unwrap_or_else(|| quote! { upcast(::std::clone::Clone::clone(&#action_ident)) });
                quote_spanned! { span => let #action_in_ident = #glib::Cast::#cast; }
            }),
        ].into_iter().flatten();
        let after = self.return_type().map(|ty| {
            let state = action
                .state_variant
                .then(|| quote! { #ret_ident })
                .unwrap_or_else(|| {
                    quote_spanned! { ty.span() =>
                        #glib::ToVariant::to_variant(&#ret_ident)
                    }
                });
            let call = activate
                .then(|| {
                    let ref_ = action_ref.is_none().then(|| quote! { & });
                    quote_spanned! { ty.span() =>
                        #go::gio::prelude::ActionExt::change_state(#ref_ #action_ident, &#state)
                    }
                })
                .unwrap_or_else(|| {
                    quote_spanned! { ty.span() =>
                        #action_ident.set_state(&#state)
                    }
                });
            let cast_ty = action.state_type(go).map(|state_ty| {
                quote_spanned! { ty.span() =>
                    let #ret_ident: #state_ty = #ret_ident;
                }
            });
            quote_spanned! { ty.span() =>
                match #ret_ident {
                    ::std::option::Option::Some(#ret_ident) => {
                        #cast_ty
                        #call
                    },
                    _ => {},
                }
            }
        });
        let arg_names = self
            .sig
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(index, arg)| {
                if Some(arg) == self.sig.receiver() {
                    return Some(this_ident);
                } else if Some(index) == self.parameter_index.map(|i| i.0) {
                    return Some(&param_ident);
                } else if Some(index) == self.action_index.map(|i| i.0) {
                    return Some(&action_in_ident);
                } else if Some(index) == self.state_index.map(|i| i.0) {
                    return Some(&state_ident);
                }
                None
            });
        let ident = &self.sig.ident;
        let async_ = self.sig.asyncness.as_ref().map(|_| quote! { async move });
        let call = quote_spanned! { self.sig.span() =>
            #self_ty::#ident(#(#arg_names),*)
        };
        let call = async_
            .as_ref()
            .map(|_| quote! { #call.await })
            .unwrap_or_else(|| call);
        let mut closure = parse_quote_spanned! { self.sig.span() =>
            move |#(#args),*| #async_ {
                #(#before)*
                let #ret_ident = #call;
                #after
            }
        };
        match &mut closure {
            syn::Expr::Closure(expr) => expr.attrs.push(parse_quote! { #[closure(local)] }),
            _ => unreachable!(),
        }
        gobject_core::closure_expr(&mut closure, go, &Errors::default());
        quote_spanned! { self.span => #closure }
    }
    fn to_public_method_expr(
        &self,
        name: &str,
        sub_ty: &TokenStream,
        wrapper_ty: &TokenStream,
        action: &Action,
        activate: bool,
        go: &syn::Ident,
    ) -> syn::Expr {
        let glib = quote! { #go::glib };
        let ident = &self.sig.ident;
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let this_ident = syn::Ident::new("obj", Span::mixed_site());
        let param_ident = syn::Ident::new("param", Span::mixed_site());
        let action_ident = syn::Ident::new("action", Span::mixed_site());
        let action_in_ident = syn::Ident::new("action", Span::mixed_site());
        let state_ident = syn::Ident::new("state", Span::mixed_site());
        let ret_ident = syn::Ident::new("_ret", Span::mixed_site());
        let await_ = self.sig.asyncness.as_ref().map(|_| quote! { .await });
        let recv_has_ref = self.sig.receiver().and_then(util::arg_reference).is_some();
        let action_ref = self
            .action_index
            .and_then(|(index, _)| util::arg_reference(self.sig.inputs.iter().nth(index)?));
        let before = [
            (action.parameter_type.is_none())
                .then(|| ())
                .and_then(|_| self.parameter_index.zip(action.parameter_type()))
                .map(|((_, span), param_ty)| quote_spanned! { span =>
                    let #param_ident: #param_ty = #param_ident;
                }),
            self.sig.receiver().and_then(|recv| {
                (self.mode == TypeMode::Subclass).then(|| {
                    let ref_ = (!recv_has_ref).then(|| quote! { & });
                    quote_spanned! { recv.span() =>
                        let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ #this_ident);
                    }
                })
            }),
            self.state_index.map(|(_, span)| {
                let unwrap = (!action.state_variant).then(|| quote_spanned! { span =>
                    let #state_ident = #glib::FromVariant::from_variant(&#state_ident)
                        .expect("Invalid state type stored in action");
                });
                let cast_ty = action.state_type(go).map(|state_ty| {
                    quote_spanned! { span =>
                        let #state_ident: #state_ty = #state_ident;
                    }
                });
                quote_spanned! { span =>
                    let #state_ident = #go::gio::prelude::ActionExt::state(&#action_ident)
                        .expect("No state stored in action");
                    #unwrap
                    #cast_ty
                }
            }),
            self.action_index.map(|(_, span)| {
                let cast = action_ref
                    .as_ref()
                    .map(|_| quote! { upcast_ref(&#action_ident) })
                    .unwrap_or_else(|| quote! { upcast(::std::clone::Clone::clone(&#action_ident)) });
                quote_spanned! { span => let #action_in_ident = #glib::Cast::#cast; }
            }),
        ].into_iter().flatten();
        let after = self.return_type().map(|ty| {
            let state = action
                .state_variant
                .then(|| quote! { #ret_ident })
                .unwrap_or_else(|| {
                    quote_spanned! { ty.span() =>
                        #glib::ToVariant::to_variant(&#ret_ident)
                    }
                });
            let call = activate
                .then(|| {
                    quote_spanned! { ty.span() =>
                        #go::gio::prelude::ActionExt::change_state(&#action_ident, &#state)
                    }
                })
                .unwrap_or_else(|| {
                    quote_spanned! { ty.span() =>
                        #action_ident.set_state(&#state)
                    }
                });
            let cast_ty = action.state_type(go).map(|state_ty| {
                quote_spanned! { ty.span() =>
                    let #ret_ident: #state_ty = #ret_ident;
                }
            });
            quote_spanned! { ty.span() =>
                match #ret_ident {
                    ::std::option::Option::Some(#ret_ident) => {
                        #cast_ty
                        #call
                    },
                    _ => {},
                }
            }
        });
        let arg_names = self
            .sig
            .inputs
            .iter()
            .enumerate()
            .filter_map(|(index, arg)| {
                if Some(arg) == self.sig.receiver() {
                    return Some(&this_ident);
                } else if Some(index) == self.parameter_index.map(|p| p.0) {
                    return Some(&param_ident);
                } else if Some(index) == self.action_index.map(|p| p.0) {
                    return Some(&action_in_ident);
                } else if Some(index) == self.state_index.map(|p| p.0) {
                    return Some(&state_ident);
                }
                None
            });
        let dest = match self.mode {
            TypeMode::Subclass => sub_ty,
            TypeMode::Wrapper => wrapper_ty,
        };

        let recv_cast = recv_has_ref
            .then(|| quote! { upcast_ref })
            .unwrap_or_else(|| quote! { upcast });
        let recv_ref = (!recv_has_ref).then(|| quote! { & });
        parse_quote_spanned! { self.span => {
            let #this_ident = #glib::Cast::#recv_cast::<#wrapper_ty>(#self_ident);
            let #action_ident = #go::gio::prelude::ActionMapExt::lookup_action(#recv_ref #this_ident, #name)
                .expect("Action not found in action map");
            let #action_ident = #glib::Cast::downcast::<#go::gio::SimpleAction>(#action_ident)
                .expect("Action not a gio::SimpleAction");
            if !#go::gio::prelude::ActionExt::is_enabled(&#action_ident) {
                return;
            }
            #(#before)*
            let #ret_ident = #dest::#ident(#(#arg_names),*) #await_;
            #after
        }}
    }
}

impl Spanned for ActionHandler {
    fn span(&self) -> Span {
        self.span.clone()
    }
}

pub(crate) struct Action {
    pub name: String,
    pub parameter_type: Option<String>,
    pub state_variant: bool,
    pub activate: Option<ActionHandler>,
    pub change_state: Option<ActionHandler>,
    pub default_state: Option<syn::Expr>,
    pub default_hint: Option<syn::Expr>,
    pub disabled: bool,
}

impl Action {
    pub(crate) fn many_from_items(
        items: &mut [syn::ImplItem],
        actions: &mut Vec<Self>,
        mode: TypeMode,
        errors: &Errors,
    ) {
        for item in items {
            if let syn::ImplItem::Method(method) = item {
                if let Some(attr) = util::extract_attr(&mut method.attrs, "action") {
                    Self::from_method(method, attr, mode, actions, errors);
                }
            }
        }
    }
    #[inline]
    fn from_method(
        method: &mut syn::ImplItemMethod,
        attr: syn::Attribute,
        mode: TypeMode,
        actions: &mut Vec<Self>,
        errors: &Errors,
    ) {
        let attrs = util::parse_paren_list::<ActionAttrs>(attr.tokens, errors);
        let sig = method.sig.clone();
        let name = attrs
            .name
            .as_ref()
            .map(|n| n.value())
            .unwrap_or_else(|| sig.ident.to_string().to_kebab_case());
        let action = if let Some(i) = actions.iter().position(|s| s.name == name) {
            &mut actions[i]
        } else {
            let action = Self {
                name,
                parameter_type: None,
                state_variant: false,
                activate: None,
                change_state: None,
                default_state: None,
                default_hint: None,
                disabled: false,
            };
            actions.push(action);
            actions.last_mut().unwrap()
        };
        if attrs.change_state.is_none() {
            if action.activate.is_some() {
                errors.push_spanned(&method.sig, "Duplicate activate handler for action");
            } else {
                action.activate = Some(ActionHandler::new(method, mode, errors));
            }
        } else if action.change_state.is_some() {
            errors.push_spanned(&method.sig, "Duplicate change-state handler for action");
        } else {
            action.change_state = Some(ActionHandler::new(method, mode, errors));
        }
        if let Some(parameter_type) = attrs.parameter_type.as_ref() {
            if action.parameter_type.is_some() {
                errors.push(
                    parameter_type.span(),
                    "Duplicate `parameter_type` attribute",
                );
            } else {
                action.parameter_type = Some(parameter_type.value());
            }
        }
        validations::only_one(
            [
                &("default_state", validations::check_spanned(&attrs.default)),
                &(
                    "default_state_variant",
                    validations::check_spanned(&attrs.default_variant),
                ),
            ],
            errors,
        );
        if let Some(default_state) = attrs.default.as_ref() {
            if action.default_state.is_none() {
                action.default_state = Some((**default_state).clone());
            }
        } else if let Some(default_state_variant) = attrs.default_variant.as_ref() {
            if action.default_state.is_none() {
                action.default_state = Some((**default_state_variant).clone());
                action.state_variant = true;
            }
        }
        if let Some(default_hint) = attrs.hint.as_ref() {
            if action.default_hint.is_some() {
                errors.push(default_hint.span(), "Duplicate `default_hint` attribute");
            } else {
                action.default_hint = Some((**default_hint).clone());
            }
        }
        if attrs.disabled.is_some() {
            if action.disabled {
                errors.push(attrs.disabled.span(), "Duplicate `disabled` attribute");
            } else {
                action.disabled = true;
            }
        }
    }
    fn parameter_type(&self) -> Option<&syn::Type> {
        self.activate
            .as_ref()
            .and_then(|h| h.parameter_type())
            .or_else(|| self.change_state.as_ref().and_then(|h| h.parameter_type()))
    }
    fn state_type(&self, go: &syn::Ident) -> Option<Cow<'_, syn::Type>> {
        self.activate
            .as_ref()
            .and_then(|h| h.state_type(go))
            .or_else(|| self.change_state.as_ref().and_then(|h| h.state_type(go)))
    }
    pub(crate) fn to_token_stream(&self, this_ident: &syn::Ident, go: &syn::Ident) -> TokenStream {
        let glib = quote! { #go::glib };
        let gio = quote! { #go::gio };
        let action_ident = syn::Ident::new("action", Span::mixed_site());
        let name = &self.name;
        let parameter_type = self
            .parameter_type
            .as_ref()
            .map(|vty| quote! { #glib::VariantTy::new(#vty).unwrap() })
            .or_else(|| {
                self.parameter_type().map(|ty| {
                    quote_spanned! { ty.span() =>
                        &*<#ty as #glib::StaticVariantType>::static_variant_type()
                    }
                })
            });
        let type_option = parameter_type
            .as_ref()
            .map(|ty| quote! { ::std::option::Option::Some(#ty) })
            .unwrap_or_else(|| quote! { ::std::option::Option::None });
        let state_ty = self.state_type(go);
        let default_state = self
            .default_state
            .as_ref()
            .map(|state| {
                quote_spanned! { state.span() => #state }
            })
            .or_else(|| {
                state_ty.as_ref().map(|state_ty| {
                    quote_spanned! { state_ty.span() =>
                        <#state_ty as ::std::default::Default>::default()
                    }
                })
            });
        let constructor = if let Some(expr) = default_state {
            let expr = state_ty
                .as_ref()
                .map(|state_ty| {
                    let state_ident = syn::Ident::new("state", Span::mixed_site());
                    quote_spanned! { expr.span() => {
                        let #state_ident: #state_ty = #expr;
                        #state_ident
                    } }
                })
                .unwrap_or_else(|| expr);
            let default_state = self
                .state_variant
                .then(|| quote! { #expr })
                .unwrap_or_else(|| {
                    quote! {
                        #glib::ToVariant::to_variant(&#expr)
                    }
                });
            quote_spanned! { expr.span() =>
                new_stateful(#name, #type_option, &#default_state)
            }
        } else {
            quote! { new(#name, #type_option) }
        };
        let activate = self.activate.as_ref().map(|handler| {
            let handler = handler.to_token_stream(this_ident, self, true, go);
            quote_spanned! { handler.span() =>
                #glib::prelude::ObjectExt::connect_closure(
                    &#action_ident,
                    "activate",
                    false,
                    #handler
                );
            }
        });
        let change_state = self.change_state.as_ref().map(|handler| {
            let handler = handler.to_token_stream(this_ident, self, false, go);
            quote_spanned! { handler.span() =>
                #glib::prelude::ObjectExt::connect_closure(
                    &#action_ident,
                    "change-state",
                    false,
                    #handler
                );
            }
        });
        let set_state_hint = self.default_hint.as_ref().map(|hint| {
            let hint = self
                .state_variant
                .then(|| quote! { #hint })
                .unwrap_or_else(|| quote! { glib::ToVariant::to_variant(&#hint) });
            quote_spanned! { hint.span() =>
                #action_ident.set_state_hint(Some(&#hint));
            }
        });
        let disable = self.disabled.then(|| {
            quote! {
                #action_ident.set_enabled(false);
            }
        });
        quote_spanned! { self.span() =>
            {
                let #action_ident = #gio::SimpleAction::#constructor;
                #activate
                #change_state
                #set_state_hint
                #disable
                #action_ident
            }
        }
    }
}

impl Spanned for Action {
    fn span(&self) -> Span {
        self.activate
            .as_ref()
            .map(|h| h.span())
            .or_else(|| self.change_state.as_ref().map(|h| h.span()))
            .unwrap_or_else(|| Span::call_site())
    }
}
