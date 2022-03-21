use crate::util;
use darling::{
    util::{Flag, IdentString},
    FromMeta,
};
use heck::{ToKebabCase, ToSnakeCase};
use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use std::collections::HashSet;
use syn::spanned::Spanned;

bitflags::bitflags! {
    pub struct SignalFlags: u32 {
        const RUN_FIRST             = 1 << 0;
        const RUN_LAST              = 1 << 1;
        const RUN_CLEANUP           = 1 << 2;
        const NO_RECURSE            = 1 << 3;
        const DETAILED              = 1 << 4;
        const ACTION                = 1 << 5;
        const NO_HOOKS              = 1 << 6;
        const MUST_COLLECT          = 1 << 7;
        const DEPRECATED            = 1 << 8;
    }
}

impl SignalFlags {
    fn tokens(&self, glib: &TokenStream) -> TokenStream {
        const COUNT: u32 =
            SignalFlags::empty().bits().leading_zeros() - SignalFlags::all().bits().leading_zeros();
        let mut flags = vec![];
        for i in 0..COUNT {
            if let Some(flag) = Self::from_bits(1 << i) {
                if self.contains(flag) {
                    let flag = format_ident!("{}", format!("{:?}", flag));
                    flags.push(quote! { #glib::SignalFlags::#flag });
                }
            }
        }
        if flags.is_empty() {
            quote! { #glib::SignalFlags::empty() }
        } else {
            quote! { #(#flags)|* }
        }
    }
}

#[derive(Default, FromMeta)]
struct SignalAttrs {
    run_first: Flag,
    run_last: Flag,
    run_cleanup: Flag,
    no_recurse: Flag,
    detailed: Flag,
    action: Flag,
    no_hooks: Flag,
    must_collect: Flag,
    deprecated: Flag,
    override_: Option<bool>,
    connect: Option<bool>,
    name: Option<IdentString>,
}

impl SignalAttrs {
    fn flags(&self) -> SignalFlags {
        let mut flags = SignalFlags::empty();
        if self.run_first.into() {
            flags |= SignalFlags::RUN_FIRST;
        }
        if self.run_last.into() {
            flags |= SignalFlags::RUN_LAST;
        }
        if self.run_cleanup.into() {
            flags |= SignalFlags::RUN_CLEANUP;
        }
        if self.no_recurse.into() {
            flags |= SignalFlags::NO_RECURSE;
        }
        if self.detailed.into() {
            flags |= SignalFlags::DETAILED;
        }
        if self.action.into() {
            flags |= SignalFlags::ACTION;
        }
        if self.no_hooks.into() {
            flags |= SignalFlags::NO_HOOKS;
        }
        if self.must_collect.into() {
            flags |= SignalFlags::MUST_COLLECT;
        }
        if self.deprecated.into() {
            flags |= SignalFlags::DEPRECATED;
        }
        flags
    }
}

pub struct Signal {
    ident: syn::Ident,
    name: String,
    flags: SignalFlags,
    connect: bool,
    override_: bool,
    handler: Option<syn::ImplItemMethod>,
    accumulator: Option<syn::ImplItemMethod>,
}

impl Signal {
    pub fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        errors: &mut Vec<darling::Error>,
    ) -> Vec<Self> {
        let mut signal_names = HashSet::new();
        let mut signals = Vec::<Signal>::new();

        let mut index = 0;
        loop {
            if index >= items.len() {
                break;
            }
            let mut signal_attr = None;
            if let syn::ImplItem::Method(method) = &mut items[index] {
                let signal_index = method.attrs.iter().position(|attr| {
                    attr.path.is_ident("signal") || attr.path.is_ident("accumulator")
                });
                if let Some(signal_index) = signal_index {
                    signal_attr.replace(method.attrs.remove(signal_index));
                }
                if let Some(next) = method.attrs.first() {
                    errors
                        .push(syn::Error::new_spanned(next, "Unknown attribute on signal").into());
                }
            }
            if let Some(attr) = signal_attr {
                let sub = items.remove(index);
                let mut method = match sub {
                    syn::ImplItem::Method(method) => method,
                    _ => unreachable!(),
                };
                if attr.path.is_ident("signal") {
                    let signal = Self::from_handler(
                        &mut method,
                        attr,
                        &mut signal_names,
                        &mut signals,
                        errors,
                    );
                    signal.handler = Some(method);
                } else if attr.path.is_ident("accumulator") {
                    let signal = Self::from_accumulator(&mut method, attr, &mut signals, errors);
                    method.sig.ident = format_ident!("accumulator");
                    signal.accumulator = Some(method);
                } else {
                    unreachable!();
                }
            } else {
                index += 1;
            }
        }

        for signal in &signals {
            if let Some(handler) = &signal.handler {
                if signal.accumulator.is_some()
                    && matches!(handler.sig.output, syn::ReturnType::Default)
                {
                    errors.push(
                        syn::Error::new_spanned(
                            handler,
                            "Signal with accumulator must have return type",
                        )
                        .into(),
                    );
                }
            } else {
                let acc = signal.accumulator.as_ref().expect("no accumulator");
                errors.push(
                    syn::Error::new_spanned(
                        acc,
                        format!("No definition for signal `{}`", signal.ident),
                    )
                    .into(),
                );
            }
            if let Some(acc) = &signal.accumulator {
                if signal.override_ {
                    errors.push(
                        syn::Error::new_spanned(acc, "Accumulator not allowed on overriden signal")
                            .into(),
                    );
                }
            }
        }

        signals
    }
    #[inline]
    fn from_handler<'signals>(
        method: &mut syn::ImplItemMethod,
        attr: syn::Attribute,
        signal_names: &mut HashSet<String>,
        signals: &'signals mut Vec<Self>,
        errors: &mut Vec<darling::Error>,
    ) -> &'signals mut Self {
        let ident = &method.sig.ident;
        if method.sig.receiver().is_none() {
            if let Some(first) = method.sig.inputs.first() {
                errors.push(
                    syn::Error::new_spanned(
                        first,
                        "First argument to signal handler must be `&self`",
                    )
                    .into(),
                );
            }
        }
        let signal_attrs = util::parse_list::<SignalAttrs>(attr.tokens.into(), errors);
        let name = signal_attrs
            .name
            .clone()
            .map(|n| n.as_str().to_owned())
            .unwrap_or_else(|| ident.to_string().to_kebab_case());
        if !util::is_valid_name(&name) {
            let name = signal_attrs
                .name
                .as_ref()
                .map(|n| n.as_ident())
                .unwrap_or_else(|| ident);
            errors.push(
                syn::Error::new_spanned(
                    name,
                    format!("Invalid signal name '{}'. Signal names must start with an ASCII letter and only contain ASCII letters, numbers, '-' or '_'", name)
                ).into()
            );
        }
        if signal_names.contains(&name) {
            errors.push(
                syn::Error::new_spanned(
                    ident,
                    format!("Duplicate definition for signal `{}`", name),
                )
                .into(),
            );
        }
        let signal = if let Some(i) = signals.iter().position(|s| s.ident == *ident) {
            &mut signals[i]
        } else {
            signals.push(Signal::new(ident.clone()));
            signals.last_mut().unwrap()
        };
        if signal.handler.is_some() {
            errors.push(
                syn::Error::new_spanned(
                    &ident,
                    format!("Duplicate definition for signal `{}`", ident),
                )
                .into(),
            );
        }
        signal_names.insert(name.clone());
        signal.name = name;
        signal.flags = signal_attrs.flags();
        signal.connect = signal_attrs.connect.unwrap_or(true);
        signal.override_ = signal_attrs.override_.unwrap_or(true);
        signal
    }
    #[inline]
    fn from_accumulator<'signals>(
        method: &mut syn::ImplItemMethod,
        attr: syn::Attribute,
        signals: &'signals mut Vec<Self>,
        errors: &mut Vec<darling::Error>,
    ) -> &'signals mut Self {
        if !attr.tokens.is_empty() {
            errors.push(
                syn::Error::new_spanned(&attr.tokens, "Unknown tokens on accumulator").into(),
            );
        }
        if !(2..=3).contains(&method.sig.inputs.len()) {
            errors.push(
                syn::Error::new_spanned(
                    &method.sig.output,
                    "Accumulator must have 2 or 3 arguments",
                )
                .into(),
            );
        }
        if let Some(recv) = method.sig.receiver() {
            errors.push(
                syn::Error::new_spanned(recv, "Receiver argument not allowed on accumulator")
                    .into(),
            );
        }
        if matches!(method.sig.output, syn::ReturnType::Default) {
            errors.push(
                syn::Error::new_spanned(&method.sig.output, "Accumulator must have return type")
                    .into(),
            );
        }
        let ident = &method.sig.ident;
        let signal = if let Some(i) = signals.iter().position(|s| s.ident == *ident) {
            &mut signals[i]
        } else {
            signals.push(Signal::new(ident.clone()));
            signals.last_mut().unwrap()
        };
        if signal.accumulator.is_some() {
            errors.push(
                syn::Error::new_spanned(
                    &ident,
                    format!(
                        "Duplicate definition for accumulator on signal definition `{}`",
                        ident
                    ),
                )
                .into(),
            );
        }
        signal
    }
    fn new(ident: syn::Ident) -> Self {
        Self {
            ident,
            name: Default::default(),
            flags: SignalFlags::empty(),
            connect: false,
            override_: false,
            handler: None,
            accumulator: None,
        }
    }
    fn inputs(&self) -> impl Iterator<Item = &syn::FnArg> + Clone {
        self.handler
            .as_ref()
            .map(|s| {
                // if override, leave the last argument for the token
                let count = s.sig.inputs.len() - if self.override_ { 1 } else { 0 };
                s.sig.inputs.iter().take(count)
            })
            .expect("no definition for signal")
    }
    fn arg_names(&self) -> impl Iterator<Item = syn::Ident> + Clone + '_ {
        self.inputs()
            .enumerate()
            .map(|(i, _)| format_ident!("arg{}", i))
    }
    fn args_unwrap<'a>(
        &'a self,
        self_ty: Option<&'a syn::Type>,
        object_type: Option<&'a syn::Type>,
        imp: bool,
        glib: &'a TokenStream,
    ) -> impl Iterator<Item = TokenStream> + 'a {
        self.inputs().enumerate().map(move |(index, input)| {
            let ty = match input {
                syn::FnArg::Receiver(_) => {
                    let self_ty = if let Some(self_ty) = self_ty {
                        quote! { #self_ty }
                    } else {
                        quote! { Self }
                    };
                    if imp {
                        if let Some(ty) = object_type {
                            quote! { #ty }
                        } else {
                            quote! { <#self_ty as #glib::subclass::types::ObjectSubclass>::Type }
                        }
                    } else {
                        quote! { #self_ty }
                    }
                }
                syn::FnArg::Typed(t) => {
                    let ty = &t.ty;
                    quote! { #ty }
                }
            };
            let arg_name = format_ident!("arg{}", index);
            let unwrap_recv = match input {
                syn::FnArg::Receiver(_) => Some(quote! {
                    let #arg_name = #glib::subclass::prelude::ObjectSubclassIsExt::imp(&#arg_name);
                }),
                _ => None,
            };
            let err_msg = format!("Wrong type for argument {}: {{:?}}", index);
            quote! {
                let #arg_name = args[#index].get::<#ty>().unwrap_or_else(|e| {
                    panic!(#err_msg, e)
                });
                #unwrap_recv
            }
        })
    }
    pub fn definition(
        &self,
        self_ty: &syn::Type,
        object_type: Option<&syn::Type>,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        if self.override_ {
            return None;
        }

        let Self {
            name,
            flags,
            handler,
            accumulator,
            ..
        } = self;

        let handler = handler.as_ref().unwrap();
        let inputs = self.inputs();
        let input_static_types = inputs.skip(1).map(|input| {
            let ty = match &input {
                syn::FnArg::Typed(t) => &t.ty,
                _ => unimplemented!(),
            };
            quote! {
                <#glib::subclass::SignalType as ::core::convert::From<#glib::Type>>::from(
                    <#ty as #glib::types::StaticType>::static_type()
                )
            }
        });
        let class_handler = (!handler.block.stmts.is_empty()).then(|| {
            let arg_names = self.arg_names();
            let args_unwrap = self.args_unwrap(Some(self_ty), object_type, true, glib);
            let method_name = &handler.sig.ident;
            quote! {
                let builder = builder.class_handler(|_, args| {
                    #(#args_unwrap)*
                    let ret = #self_ty::#method_name(#(#arg_names),*);
                    #glib::closure::ToClosureReturnValue::to_closure_return_value(&ret)
                });
            }
        });
        let output = match &handler.sig.output {
            syn::ReturnType::Type(_, ty) => quote! { #ty },
            _ => quote! { () },
        };
        let accumulator = accumulator.as_ref().map(|method| {
            let ident = &method.sig.ident;
            let call_args = if method.sig.inputs.len() == 2 {
                quote! { &mut output, value }
            } else {
                quote! { _hint, &mut output, value }
            };
            quote! {
                let builder = builder.accumulator(|_hint, accu, value| {
                    #method
                    let curr_accu = accu.get().unwrap();
                    let value = value.get().unwrap();
                    let (next, ret) = match #ident(#call_args) {
                        ::std::ops::ControlFlow::Continue(next) => (next, true),
                        ::std::ops::ControlFlow::Break(next) => (next, false),
                    };
                    if let ::std::option::Some(next) = next {
                        *accu = #glib::ToValue::to_value(&next);
                    }
                    ret
                });
            }
        });
        let flags = (!flags.is_empty()).then(|| {
            let flags = flags.tokens(glib);
            quote! { let builder = builder.flags(#flags); }
        });
        Some(quote_spanned! { handler.span() =>
            {
                let param_types = [#(#input_static_types),*];
                let builder = #glib::subclass::Signal::builder(
                    #name,
                    &param_types,
                    <#glib::subclass::SignalType as ::core::convert::From<#glib::Type>>::from(
                        <#output as #glib::types::StaticType>::static_type()
                    ),
                );
                #flags
                #class_handler
                #accumulator
                builder.build()
            }
        })
    }
    pub fn class_init_override(
        &self,
        self_ty: &syn::Type,
        object_type: Option<&syn::Type>,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        if !self.override_ {
            return None;
        }
        let arg_names = self.arg_names();
        let args_unwrap = self.args_unwrap(Some(self_ty), object_type, true, glib);
        let method_name = &self.handler.as_ref().unwrap().sig.ident;
        Some(quote! {
            #glib::subclass::object::ObjectClassSubclassExt::override_signal_class_handler(
                klass,
                |token, values| {
                    #(#args_unwrap)*
                    let ret = #self_ty::#method_name(#(#arg_names,)* token);
                    #glib::closure::ToClosureReturnValue::to_closure_return_value(&ret)
                }
            );
        })
    }
    pub fn handler_definition(&self) -> Option<TokenStream> {
        let handler = self.handler.as_ref().unwrap();
        if !handler.block.stmts.is_empty() {
            Some(quote_spanned! { handler.span() =>
                #handler
            })
        } else {
            None
        }
    }
    fn emit_arg_defs(&self) -> impl Iterator<Item = syn::PatType> + Clone + '_ {
        self.inputs().skip(1).enumerate().map(|(index, arg)| {
            let mut ty = match arg {
                syn::FnArg::Typed(t) => t,
                _ => unimplemented!(),
            }
            .clone();
            let pat_ident = Box::new(syn::Pat::Ident(syn::PatIdent {
                attrs: vec![],
                by_ref: None,
                mutability: None,
                ident: format_ident!("arg{}", index),
                subpat: None,
            }));
            if !matches!(&*ty.pat, syn::Pat::Ident(_)) {
                ty.pat = pat_ident;
            }
            ty
        })
    }
    pub fn is_action(&self) -> bool {
        self.flags.contains(SignalFlags::ACTION)
    }
    pub fn signal_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        if self.override_ {
            return None;
        }
        let method_name = format_ident!("signal_{}", self.name.to_snake_case());
        Some(quote_spanned! { self.handler.as_ref().unwrap().span() =>
            fn #method_name() -> &'static #glib::subclass::Signal
        })
    }
    pub fn signal_definition(
        &self,
        index: usize,
        signals_path: &TokenStream,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        let proto = self.signal_prototype(glib)?;
        Some(quote_spanned! { self.handler.as_ref().unwrap().span() =>
            #proto {
                #![inline]
                &#signals_path()[#index]
            }
        })
    }
    pub fn emit_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        if self.override_ {
            return None;
        }
        let handler = self.handler.as_ref().unwrap();
        let output = &handler.sig.output;
        let method_name = format_ident!("emit_{}", self.name.to_snake_case());
        let arg_defs = self.emit_arg_defs();
        let details_arg = self
            .flags
            .contains(SignalFlags::DETAILED)
            .then(|| quote! { signal_details: ::std::option::Option<#glib::Quark>, });
        Some(quote_spanned! { handler.span() =>
            fn #method_name(&self, #details_arg #(#arg_defs),*) #output
        })
    }
    pub fn emit_definition(
        &self,
        index: usize,
        signals_path: &TokenStream,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        let proto = self.emit_prototype(glib)?;
        let handler = self.handler.as_ref().unwrap();
        let arg_defs = self.emit_arg_defs();
        let arg_names = arg_defs.clone().map(|arg| match &*arg.pat {
            syn::Pat::Ident(syn::PatIdent { ident, .. }) => ident.clone(),
            _ => unimplemented!(),
        });
        let signal_id = quote! { #signals_path()[#index].signal_id() };
        let emit = {
            let arg_names = arg_names.clone();
            quote! {
                <Self as #glib::object::ObjectExt>::emit(
                    self,
                    #signal_id,
                    &[#(&#arg_names),*]
                )
            }
        };
        let body = if self.flags.contains(SignalFlags::DETAILED) {
            quote! {
                if let Some(signal_details) = signal_details {
                    <Self as #glib::object::ObjectExt>::emit_with_details(
                        self,
                        #signal_id,
                        signal_details,
                        &[#(&#arg_names),*]
                    )
                } else {
                    #emit
                }
            }
        } else {
            emit
        };
        Some(quote_spanned! { handler.span() =>
            #proto {
                #![inline]
                #body
            }
        })
    }
    pub fn connect_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        if !self.connect || self.override_ {
            return None;
        }
        let method_name = format_ident!("connect_{}", self.name.to_snake_case());
        let handler = self.handler.as_ref().unwrap();
        let output = &handler.sig.output;
        let input_types = self.inputs().skip(1).map(|arg| match arg {
            syn::FnArg::Typed(t) => &t.ty,
            _ => unimplemented!(),
        });
        let details_arg = self
            .flags
            .contains(SignalFlags::DETAILED)
            .then(|| quote! { details: ::std::option::Option<#glib::Quark>, });
        Some(quote_spanned! { handler.span() =>
            fn #method_name<F: Fn(&Self, #(#input_types),*) #output + 'static>(
                &self,
                #details_arg
                f: F,
            ) -> #glib::SignalHandlerId
        })
    }
    pub fn connect_definition(
        &self,
        index: usize,
        signals_path: &TokenStream,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        let proto = self.connect_prototype(glib)?;
        let handler = self.handler.as_ref().unwrap();
        let arg_names = self.arg_names().skip(1);
        let args_unwrap = self.args_unwrap(None, None, false, glib).skip(1);

        let details = if self.flags.contains(SignalFlags::DETAILED) {
            quote! { details }
        } else {
            quote! { ::std::option::Option::None }
        };

        let unwrap = match &handler.sig.output {
            syn::ReturnType::Type(_, _) => quote! {
                #glib::closure::ToClosureReturnValue::to_closure_return_value(&_ret)
            },
            _ => quote! { ::core::option::Option::None },
        };
        Some(quote_spanned! { handler.span() =>
            #proto {
                #![inline]
                <Self as #glib::object::ObjectExt>::connect_local_id(
                    self,
                    #signals_path()[#index].signal_id(),
                    #details,
                    false,
                    move |args| {
                        let recv = args[0].get::<Self>().unwrap();
                        #(#args_unwrap)*
                        let _ret = f(&recv, #(#arg_names),*);
                        #unwrap
                    },
                )
            }
        })
    }
}
