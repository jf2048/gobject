use darling::{
    util::{Flag, SpannedValue},
    FromAttributes, FromMeta,
};
use gobject_core::{
    util::{self, Errors},
    validations, PublicMethod, TypeContext, TypeMode,
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
    for action in &actions {
        action.override_public_methods(None, def, errors);
    }
    let go = &def.inner.crate_ident;
    let this_ident = syn::Ident::new("obj", Span::mixed_site());
    let actions = actions.iter().map(|action| {
        let action = action.to_token_stream(&this_ident, true, go);
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

#[cfg(feature = "gio")]
pub fn impl_actions(
    mut impl_: syn::ItemImpl,
    attrs: TokenStream,
    go: &syn::Ident,
    errors: &Errors,
) -> TokenStream {
    #[derive(Default, FromMeta)]
    #[darling(default)]
    struct ActionsAttrs {
        register: Option<syn::LitStr>,
    }

    let attrs = util::parse_list::<ActionsAttrs>(attrs, errors);
    let mut actions = Vec::new();
    Action::many_from_items(&mut impl_.items, &mut actions, TypeMode::Wrapper, errors);
    validate_actions(&actions, errors);
    let ty = &impl_.self_ty;
    let (impl_generics, _, where_clause) = impl_.generics.split_for_impl();
    let register_func = attrs
        .register
        .map(|r| syn::Ident::new(&r.value(), r.span()))
        .unwrap_or_else(|| syn::Ident::new("register_actions", Span::call_site()));
    let self_ident = syn::Ident::new("self", Span::mixed_site());
    let this_ident = syn::Ident::new("this", Span::mixed_site());
    let group_ident = syn::Ident::new("group", Span::mixed_site());
    let actions = actions.iter().map(|action| {
        let action = action.to_token_stream(&this_ident, false, go);
        quote! { #go::gio::prelude::ActionMapExt::add_action(#group_ident, &#action); }
    });
    quote! {
        #impl_
        impl #impl_generics #ty #where_clause {
            fn #register_func(&#self_ident, #group_ident: &impl #go::glib::IsA<#go::gio::ActionMap>) {
                let #this_ident = #self_ident;
                #(#actions)*
            }
        }
    }
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

#[derive(Default, FromAttributes)]
#[darling(default, attributes(action))]
struct ActionAttrs {
    name: Option<syn::LitStr>,
    parameter_type: Option<SpannedValue<ParameterType>>,
    change_state: SpannedValue<Flag>,
    default: Option<SpannedValue<syn::Expr>>,
    default_variant: Option<SpannedValue<syn::Expr>>,
    hint: Option<SpannedValue<syn::Expr>>,
    disabled: SpannedValue<Flag>,
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
pub(crate) enum HandlerType {
    Activate,
    ChangeState,
}

pub(crate) struct ActionHandler {
    pub span: Span,
    pub sig: syn::Signature,
    pub mode: TypeMode,
    pub ty: HandlerType,
    pub parameter_index: Option<(usize, Span)>,
    pub state_index: Option<(usize, Span)>,
    pub action_index: Option<(usize, Span)>,
}

impl ActionHandler {
    fn new(
        method: &mut syn::ImplItemMethod,
        mode: TypeMode,
        ty: HandlerType,
        errors: &Errors,
    ) -> Self {
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
                util::require_empty(&attr, errors);
                if state_index.is_some() {
                    errors.push_spanned(&attr, "Duplicate state argument");
                } else {
                    state_index = Some((index, arg.span()));
                }
            } else if let Some(attr) = util::extract_attr(&mut arg.attrs, "action") {
                util::require_empty(&attr, errors);
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
        if ty == HandlerType::ChangeState && parameter_index.is_none() {
            errors.push_spanned(
                &method.sig,
                "Change state handler must have a parameter to receive the new state",
            );
        }
        let mut handler = Self {
            span: method.span(),
            sig: method.sig.clone(),
            mode,
            ty,
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
    fn parameter_type(&self, action: &Action) -> Option<&syn::Type> {
        if self.ty != HandlerType::Activate
            && !matches!(&action.parameter_type, ParameterType::Inferred)
        {
            return None;
        }
        self.parameter_index.and_then(|(index, _)| {
            let param = self.sig.inputs.iter().nth(index);
            match param {
                Some(syn::FnArg::Typed(p)) => Some(&*p.ty),
                _ => None,
            }
        })
    }
    fn state_type(&self, go: &syn::Ident) -> Option<Cow<'_, syn::Type>> {
        (self.ty == HandlerType::ChangeState)
            .then(|| {
                self.parameter_index.and_then(|(index, _)| {
                    let param = self.sig.inputs.iter().nth(index);
                    match param {
                        Some(syn::FnArg::Typed(p)) => Some(Cow::Borrowed(&*p.ty)),
                        _ => None,
                    }
                })
            })
            .flatten()
            .or_else(|| {
                self.state_index
                    .and_then(|(index, _)| {
                        let param = self.sig.inputs.iter().nth(index);
                        match param {
                            Some(syn::FnArg::Typed(p)) => Some(Cow::Borrowed(&*p.ty)),
                            _ => None,
                        }
                    })
                    .or_else(|| {
                        self.return_type().map(|ty| {
                            Cow::Owned(parse_quote_spanned! { ty.span() =>
                                <#ty as #go::ActionStateReturn>::ReturnType
                            })
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
    pub(crate) fn param_needs_convert(&self, action: &Action) -> bool {
        match self.ty {
            HandlerType::Activate => matches!(&action.parameter_type, ParameterType::Inferred),
            HandlerType::ChangeState => !action.state_variant,
        }
    }
    fn to_signal_closure(
        &self,
        action: &Action,
        this_ident: &syn::Ident,
        is_object: bool,
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
            self.sig.receiver().map(|_| {
                if is_object {
                    quote! { #[watch] #this_ident }
                } else {
                    quote! { #[weak(or_panic)] #this_ident }
                }
            }),
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
            (!matches!(&action.parameter_type, ParameterType::Empty))
                .then(|| ())
                .and(self.parameter_index)
                .map(|(_, span)| {
                    let param_ty = match self.ty {
                        HandlerType::Activate => action.parameter_type().map(Cow::Borrowed),
                        HandlerType::ChangeState => action.state_type(go),
                    };
                    let cast_ty = param_ty.map(|param_ty| quote_spanned! { span =>
                        let #param_ident: #param_ty = #param_ident;
                    });
                    let convert = self.param_needs_convert(action).then(|| quote_spanned! { span =>
                        let #param_ident = #glib::FromVariant::from_variant(&#param_ident)
                            .expect("Invalid type passed for action parameter");
                    });
                    quote_spanned! { span =>
                        #convert
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
            let call = match self.ty {
                HandlerType::Activate => {
                    let ref_ = action_ref.is_none().then(|| quote! { & });
                    quote_spanned! { ty.span() =>
                        #go::gio::prelude::ActionExt::change_state(#ref_ #action_ident, &#state);
                    }
                }
                HandlerType::ChangeState => quote_spanned! { ty.span() =>
                    #action_ident.set_state(&#state);
                },
            };
            let cast_ty = action.state_type(go).map(|state_ty| {
                quote_spanned! { ty.span() =>
                    let #ret_ident: #state_ty = #ret_ident;
                }
            });
            quote_spanned! { ty.span() =>
                if let ::std::option::Option::Some(#ret_ident) = #ret_ident {
                    #cast_ty
                    #call
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
        action: &Action,
        sub_ty: &TokenStream,
        wrapper_ty: &TokenStream,
        bind_expr: Option<&syn::Expr>,
        go: &syn::Ident,
    ) -> syn::Expr {
        let glib = quote! { #go::glib };
        let ident = &self.sig.ident;
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let name = &action.name;
        let recv = match self.sig.receiver() {
            Some(recv) => recv,
            None => match self.ty {
                HandlerType::Activate => {
                    let param = self
                        .parameter_index
                        .map(|(_, span)| {
                            let param_ident = syn::Ident::new("param", Span::mixed_site());
                            let param = self
                                .param_needs_convert(action)
                                .then(|| {
                                    quote_spanned! { self.sig.span() =>
                                        #go::glib::ToVariant::to_variant(&#param_ident)
                                    }
                                })
                                .unwrap_or_else(|| quote! { #param_ident });
                            quote_spanned! { span => ::std::option::Option::Some(&#param) }
                        })
                        .unwrap_or_else(|| quote! { ::std::option::Option::None });
                    return parse_quote_spanned! { self.sig.span() => {
                        #go::gio::prelude::ActionGroupExt::activate_action(
                            #self_ident
                            #name,
                            #param,
                        );
                    }};
                }
                HandlerType::ChangeState => {
                    if self.parameter_index.is_none() {
                        return parse_quote! {{}};
                    }
                    let param_ident = syn::Ident::new("param", Span::mixed_site());
                    let param = self
                        .param_needs_convert(action)
                        .then(|| {
                            quote_spanned! { self.sig.span() =>
                                #go::glib::ToVariant::to_variant(&#param_ident)
                            }
                        })
                        .unwrap_or_else(|| quote! { #param_ident });
                    return parse_quote_spanned! { self.sig.span() => {
                        #go::gio::prelude::ActionGroupExt::change_action_state(
                            #self_ident
                            #name,
                            &#param,
                        );
                    }};
                }
            },
        };
        let this_ident = syn::Ident::new("obj", Span::mixed_site());
        let param_ident = syn::Ident::new("param", Span::mixed_site());
        let action_ident = syn::Ident::new("action", Span::mixed_site());
        let action_in_ident = syn::Ident::new("action", Span::mixed_site());
        let state_ident = syn::Ident::new("state", Span::mixed_site());
        let ret_ident = syn::Ident::new("_ret", Span::mixed_site());
        let await_ = self.sig.asyncness.as_ref().map(|_| quote! { .await });
        let recv_has_ref = util::arg_reference(recv).is_some();
        let action_ref = self
            .action_index
            .and_then(|(index, _)| util::arg_reference(self.sig.inputs.iter().nth(index)?));
        let before = [
            (!matches!(&action.parameter_type, ParameterType::Empty))
                .then(|| ())
                .and_then(|_| self.parameter_index.zip(match self.ty {
                    HandlerType::Activate => action.parameter_type().map(Cow::Borrowed),
                    HandlerType::ChangeState => action.state_type(go),
                }))
                .map(|((_, span), param_ty)| quote_spanned! { span =>
                    let #param_ident: #param_ty = #param_ident;
                }),
            (self.mode == TypeMode::Subclass).then(|| {
                let ref_ = (!recv_has_ref).then(|| quote! { & });
                quote_spanned! { recv.span() =>
                    let #this_ident = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ #this_ident);
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
            let call = match self.ty {
                HandlerType::Activate => quote_spanned! { ty.span() =>
                    #go::gio::prelude::ActionExt::change_state(&#action_ident, &#state);
                },
                HandlerType::ChangeState => quote_spanned! { ty.span() =>
                    #action_ident.set_state(&#state);
                },
            };
            let cast_ty = action.state_type(go).map(|state_ty| {
                quote_spanned! { ty.span() =>
                    let #ret_ident: #state_ty = #ret_ident;
                }
            });
            quote_spanned! { ty.span() =>
                if let ::std::option::Option::Some(#ret_ident) = #ret_ident {
                    #cast_ty
                    #call
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
        let action_map = quote_spanned! { Span::mixed_site() => #recv_ref #this_ident };
        let action_map = bind_expr
            .map(|expr| {
                let group_ident = syn::Ident::new("group", Span::mixed_site());
                quote_spanned! { expr.span() =>
                    &match #go::ParamStoreReadOptional::get_owned_optional(
                        &#glib::subclass::prelude::ObjectSubclassIsExt::imp(#action_map).#expr,
                    ) {
                        ::std::option::Option::Some(#group_ident) => #group_ident,
                        _ => return,
                    }
                }
            })
            .unwrap_or_else(|| action_map);
        parse_quote_spanned! { self.span => {
            let #this_ident = #glib::Cast::#recv_cast::<#wrapper_ty>(#self_ident);
            let #action_ident = #go::gio::prelude::ActionMapExt::lookup_action(#action_map, #name)
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
        self.span
    }
}

#[derive(Clone)]
pub(crate) enum ParameterType {
    Empty,
    Inferred,
    String(syn::LitStr),
}

impl Default for ParameterType {
    fn default() -> Self {
        Self::Inferred
    }
}

impl darling::FromMeta for ParameterType {
    fn from_value(lit: &syn::Lit) -> darling::Result<Self> {
        match lit {
            syn::Lit::Str(lit) => Ok(Self::String(lit.clone())),
            syn::Lit::Bool(syn::LitBool { value, .. }) => {
                if *value {
                    Ok(Self::Inferred)
                } else {
                    Ok(Self::Empty)
                }
            }
            _ => Err(darling::Error::unexpected_lit_type(lit)),
        }
    }
}

pub(crate) struct Action {
    pub name: String,
    pub parameter_type: ParameterType,
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
                if let Some(attrs) = util::extract_attrs(&mut method.attrs, "action") {
                    let attr = util::parse_attributes::<ActionAttrs>(&attrs, errors);
                    Self::from_method(method, attr, mode, actions, errors);
                }
            }
        }
    }
    #[inline]
    fn from_method(
        method: &mut syn::ImplItemMethod,
        attr: ActionAttrs,
        mode: TypeMode,
        actions: &mut Vec<Self>,
        errors: &Errors,
    ) {
        let sig = method.sig.clone();
        let name = attr
            .name
            .as_ref()
            .map(|n| n.value())
            .unwrap_or_else(|| sig.ident.to_string().to_kebab_case());
        let action = if let Some(i) = actions.iter().position(|s| s.name == name) {
            &mut actions[i]
        } else {
            let action = Self {
                name,
                parameter_type: ParameterType::default(),
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
        if attr.change_state.is_none() {
            if action.activate.is_some() {
                errors.push_spanned(&method.sig, "Duplicate activate handler for action");
            } else {
                action.activate = Some(ActionHandler::new(
                    method,
                    mode,
                    HandlerType::Activate,
                    errors,
                ));
            }
        } else if action.change_state.is_some() {
            errors.push_spanned(&method.sig, "Duplicate change-state handler for action");
        } else {
            action.change_state = Some(ActionHandler::new(
                method,
                mode,
                HandlerType::ChangeState,
                errors,
            ));
        }
        if let Some(parameter_type) = attr.parameter_type {
            if !matches!(action.parameter_type, ParameterType::Inferred) {
                errors.push(
                    parameter_type.span(),
                    "Conflicting `parameter_type` attribute",
                );
            } else {
                action.parameter_type = (*parameter_type).clone();
            }
        }
        validations::only_one(
            [
                &("default_state", validations::check_spanned(&attr.default)),
                &(
                    "default_state_variant",
                    validations::check_spanned(&attr.default_variant),
                ),
            ],
            errors,
        );
        if let Some(default_state) = attr.default {
            if action.default_state.is_none() {
                action.default_state = Some((*default_state).clone());
            }
        } else if let Some(default_state_variant) = attr.default_variant {
            if action.default_state.is_none() {
                action.default_state = Some((*default_state_variant).clone());
                action.state_variant = true;
            }
        }
        if let Some(default_hint) = attr.hint {
            if action.default_hint.is_some() {
                errors.push(default_hint.span(), "Duplicate `default_hint` attribute");
            } else {
                action.default_hint = Some((*default_hint).clone());
            }
        }
        if attr.disabled.is_some() {
            if action.disabled {
                errors.push(attr.disabled.span(), "Duplicate `disabled` attribute");
            } else {
                action.disabled = true;
            }
        }
    }
    fn parameter_type(&self) -> Option<&syn::Type> {
        self.activate
            .as_ref()
            .and_then(|h| h.parameter_type(self))
            .or_else(|| {
                self.change_state
                    .as_ref()
                    .and_then(|h| h.parameter_type(self))
            })
    }
    fn state_type(&self, go: &syn::Ident) -> Option<Cow<'_, syn::Type>> {
        self.activate
            .as_ref()
            .and_then(|h| h.state_type(go))
            .or_else(|| self.change_state.as_ref().and_then(|h| h.state_type(go)))
    }
    pub(crate) fn override_public_methods(
        &self,
        bind_expr: Option<&syn::Expr>,
        def: &mut gobject_core::ClassDefinition,
        errors: &Errors,
    ) {
        use HandlerType::*;
        use TypeContext::*;
        use TypeMode::*;
        let (sub_ty, wrapper_ty) = match def
            .inner
            .type_(Subclass, Subclass, External)
            .zip(def.inner.type_(Subclass, Wrapper, External))
        {
            Some(tys) => tys,
            _ => return,
        };
        self.override_public_method(Activate, &sub_ty, &wrapper_ty, bind_expr, def, errors);
        self.override_public_method(ChangeState, &sub_ty, &wrapper_ty, bind_expr, def, errors);
    }
    pub(crate) fn prepare_public_method(
        &self,
        handler: &ActionHandler,
        public_method: &mut PublicMethod,
        final_: bool,
        errors: &Errors,
    ) {
        if handler.mode == TypeMode::Wrapper
            && !matches!(
                &public_method.constructor,
                Some(gobject_core::ConstructorType::Auto(_, _))
            )
            && public_method.target.is_none()
            && final_
        {
            errors.push(
                handler.sig.span(),
                "action using #[public] on wrapper type for final class must be renamed with #[public(name = \"...\")]",
            );
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
                    if let syn::FnArg::Typed(ty) = &mut arg {
                        ty.pat = parse_quote_spanned! { Span::mixed_site() => param };
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
        } else {
            public_method.sig.asyncness = None;
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
    fn override_public_method(
        &self,
        handler_type: HandlerType,
        sub_ty: &TokenStream,
        wrapper_ty: &TokenStream,
        bind_expr: Option<&syn::Expr>,
        def: &mut gobject_core::ClassDefinition,
        errors: &Errors,
    ) -> Option<()> {
        let handler = match handler_type {
            HandlerType::Activate => self.activate.as_ref()?,
            HandlerType::ChangeState => self.change_state.as_ref()?,
        };
        let go = def.inner.crate_ident.clone();
        let public_method = def
            .inner
            .public_method_mut(handler.mode, &handler.sig.ident)?;
        self.prepare_public_method(handler, public_method, def.final_, errors);
        public_method.custom_body = Some((
            String::from("#[action]"),
            Box::new(handler.to_public_method_expr(self, sub_ty, wrapper_ty, bind_expr, &go)),
        ));
        Some(())
    }
    pub(crate) fn to_token_stream(
        &self,
        this_ident: &syn::Ident,
        is_object: bool,
        go: &syn::Ident,
    ) -> TokenStream {
        let glib = quote! { #go::glib };
        let gio = quote! { #go::gio };
        let action_ident = syn::Ident::new("action", Span::mixed_site());
        let name = &self.name;
        let parameter_type = match &self.parameter_type {
            ParameterType::Empty => None,
            ParameterType::Inferred => self.parameter_type().map(|ty| {
                quote_spanned! { ty.span() =>
                    &*<#ty as #glib::StaticVariantType>::static_variant_type()
                }
            }),
            ParameterType::String(vty) => Some(quote_spanned! { vty.span() =>
                #glib::VariantTy::new(#vty).unwrap()
            }),
        };
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
            let handler = handler.to_signal_closure(self, this_ident, is_object, go);
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
            let handler = handler.to_signal_closure(self, this_ident, is_object, go);
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
            .unwrap_or_else(Span::call_site)
    }
}
