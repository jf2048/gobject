use crate::{util, TypeBase};
use darling::{
    util::{Flag, IdentString},
    FromMeta,
};
use heck::{ToKebabCase, ToSnakeCase, ToShoutySnakeCase};
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
#[darling(default)]
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
    override_: Flag,
    connect: Option<bool>,
    name: Option<IdentString>,
}

impl SignalAttrs {
    fn flags(&self) -> SignalFlags {
        let mut flags = SignalFlags::empty();
        flags.set(SignalFlags::RUN_FIRST,    self.run_first.is_some());
        flags.set(SignalFlags::RUN_LAST,     self.run_last.is_some());
        flags.set(SignalFlags::RUN_CLEANUP,  self.run_cleanup.is_some());
        flags.set(SignalFlags::NO_RECURSE,   self.no_recurse.is_some());
        flags.set(SignalFlags::DETAILED,     self.detailed.is_some());
        flags.set(SignalFlags::ACTION,       self.action.is_some());
        flags.set(SignalFlags::NO_HOOKS,     self.no_hooks.is_some());
        flags.set(SignalFlags::MUST_COLLECT, self.must_collect.is_some());
        flags.set(SignalFlags::DEPRECATED,   self.deprecated.is_some());
        flags
    }
}

#[derive(Debug)]
pub struct Signal {
    pub ident: syn::Ident,
    pub name: String,
    pub flags: SignalFlags,
    pub connect: bool,
    pub override_: bool,
    pub handler: Option<syn::ImplItemMethod>,
    pub accumulator: Option<syn::ImplItemMethod>,
}

impl Signal {
    pub(crate) fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        base: TypeBase,
        errors: &mut Vec<darling::Error>,
    ) -> Vec<Self> {
        let mut signal_names = HashSet::new();
        let mut signals = Vec::<Signal>::new();

        let mut index = 0;
        while index < items.len() {
            let mut signal_attr = None;
            if let syn::ImplItem::Method(method) = &mut items[index] {
                let signal_index = method.attrs.iter().position(|attr| {
                    attr.path.is_ident("signal") || attr.path.is_ident("accumulator")
                });
                if let Some(signal_index) = signal_index {
                    signal_attr.replace(method.attrs.remove(signal_index));
                }
                if let Some(next) = method.attrs.first() {
                    util::push_error_spanned(
                        errors,
                        next,
                        "Unknown attribute on signal",
                    );
                }
            }
            if let Some(attr) = signal_attr {
                let method = match &items[index] {
                    syn::ImplItem::Method(method) => method.clone(),
                    _ => unreachable!(),
                };
                if attr.path.is_ident("signal") {
                    Self::from_handler(
                        method,
                        attr,
                        base,
                        &mut signal_names,
                        &mut signals,
                        errors,
                    );
                } else if attr.path.is_ident("accumulator") {
                    let method = method.clone();
                    Self::from_accumulator(method, attr, &mut signals, errors);
                } else {
                    unreachable!();
                }
            } else {
                index += 1;
            }
        }

        for signal in &mut signals {
            if let Some(handler) = &signal.handler {
                if signal.accumulator.is_some()
                    && matches!(handler.sig.output, syn::ReturnType::Default)
                {
                    util::push_error_spanned(
                        errors,
                        handler,
                        "Signal with accumulator must have return type",
                    );
                }
            } else {
                let acc = signal.accumulator.as_ref().expect("no accumulator");
                util::push_error_spanned(
                    errors,
                    acc,
                    format!("No definition for signal `{}`", signal.ident),
                );
            }
            if let Some(acc) = &signal.accumulator {
                if signal.override_ {
                    util::push_error_spanned(
                        errors,
                        acc,
                        "Accumulator not allowed on overriden signal"
                    );
                }
            }
            if base == TypeBase::Interface && signal.override_ {
                util::push_error_spanned(
                    errors,
                    &signal.ident,
                    "`override` not allowed on interface signal"
                );
                signal.override_ = false;
            }
        }

        signals
    }
    #[inline]
    fn from_handler(
        method: syn::ImplItemMethod,
        attr: syn::Attribute,
        base: TypeBase,
        signal_names: &mut HashSet<String>,
        signals: &mut Vec<Self>,
        errors: &mut Vec<darling::Error>,
    ) {
        let ident = &method.sig.ident;
        match base {
            TypeBase::Class => if method.sig.receiver().is_none() {
                if let Some(first) = method.sig.inputs.first() {
                    util::push_error_spanned(
                        errors,
                        first,
                        "First argument to class signal handler must be `&self`",
                    );
                }
            },
            TypeBase::Interface => if let Some(recv) = method.sig.receiver() {
                util::push_error_spanned(
                    errors,
                    recv,
                    "First argument to interface signal handler must be the wrapper type",
                );
            },
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
            util::push_error_spanned(
                errors,
                name,
                format!("Invalid signal name '{}'. Signal names must start with an ASCII letter and only contain ASCII letters, numbers, '-' or '_'", name)
            );
        }
        if signal_names.contains(&name) {
            util::push_error_spanned(
                errors,
                ident,
                format!("Duplicate definition for signal `{}`", name),
            );
        }
        let signal = if let Some(i) = signals.iter().position(|s| s.ident == *ident) {
            &mut signals[i]
        } else {
            signals.push(Signal::new(ident.clone()));
            signals.last_mut().unwrap()
        };
        if signal.handler.is_some() {
            util::push_error_spanned(
                errors,
                &ident,
                format!("Duplicate definition for signal `{}`", ident),
            );
        }
        signal_names.insert(name.clone());
        signal.name = name;
        signal.flags = signal_attrs.flags();
        signal.connect = signal_attrs.connect.unwrap_or(true);
        signal.override_ = signal_attrs.override_.is_some();
        signal.handler = Some(method);
    }
    #[inline]
    fn from_accumulator(
        method: syn::ImplItemMethod,
        attr: syn::Attribute,
        signals: &mut Vec<Self>,
        errors: &mut Vec<darling::Error>,
    ) {
        if !attr.tokens.is_empty() {
            util::push_error_spanned(
                errors,
                &attr.tokens,
                "Unknown tokens on accumulator",
            );
        }
        if !(2..=3).contains(&method.sig.inputs.len()) {
            util::push_error_spanned(
                errors,
                &method.sig.output,
                "Accumulator must have 2 or 3 arguments",
            );
        }
        if let Some(recv) = method.sig.receiver() {
            util::push_error_spanned(
                errors,
                recv,
                "Receiver argument not allowed on accumulator"
            );
        }
        if matches!(method.sig.output, syn::ReturnType::Default) {
            util::push_error_spanned(
                errors,
                &method.sig.output,
                "Accumulator must have return type"
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
            util::push_error_spanned(
                errors,
                &ident,
                format!(
                    "Duplicate definition for accumulator on signal definition `{}`",
                    ident
                ),
            );
        }
        signal.accumulator = Some(method);
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
            /*
            .map(|s| {
                // if override, leave the last argument for the token
                let count = s.sig.inputs.len() - if self.override_ { 1 } else { 0 };
                s.sig.inputs.iter().take(s.sig.inputs.len())
            })
            */
            .map(|s| s.sig.inputs.iter().take(s.sig.inputs.len()))
            .expect("no definition for signal")
    }
    fn arg_names(&self) -> impl Iterator<Item = syn::Ident> + Clone + '_ {
        self.inputs()
            .enumerate()
            .map(|(i, _)| format_ident!("arg{}", i))
    }
    fn args_unwrap<'a>(
        &'a self,
        self_ty: &'a TokenStream,
        glib: &'a TokenStream,
    ) -> impl Iterator<Item = TokenStream> + 'a {
        self.inputs().enumerate().map(move |(index, input)| {
            let ty = match input {
                syn::FnArg::Receiver(_) => {
                    quote! { #self_ty }
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
    fn arg_types(&self) -> impl Iterator<Item = syn::PatType> + Clone + '_ {
        self.inputs().skip(1).enumerate().map(|(index, arg)| {
            let mut ty = match arg {
                syn::FnArg::Typed(t) => t,
                _ => unimplemented!(),
            }
            .clone();
            if !matches!(&*ty.pat, syn::Pat::Ident(_)) {
                ty.pat = Box::new(syn::Pat::Ident(syn::PatIdent {
                    attrs: vec![],
                    by_ref: None,
                    mutability: None,
                    ident: format_ident!("arg{}", index),
                    subpat: None,
                }));
            }
            ty
        })
    }
    fn signal_id_cell_ident(&self) -> syn::Ident {
        format_ident!("{}_SIGNAL", self.name.to_shouty_snake_case())
    }
    pub(crate) fn signal_id_cell_definition(&self, wrapper_ty: &TokenStream, glib: &TokenStream) -> TokenStream {
        let name = &self.name;
        let ident = self.signal_id_cell_ident();
        quote! {
            static #ident: #glib::once_cell::sync::Lazy<#glib::subclass::SignalId>> =
                #glib::once_cell::sync::Lazy::new(|| {
                    #glib::subclass::SignalId::lookup(
                        #name,
                        <#wrapper_ty as #glib::StaticType>::static_type(),
                    ).expect()
                });
        }
    }
    pub(crate) fn definition(
        &self,
        wrapper_ty: &TokenStream,
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
            let args_unwrap = self.args_unwrap(wrapper_ty, glib);
            let method_name = &handler.sig.ident;
            quote! {
                let builder = builder.class_handler(|_, args| {
                    #(#args_unwrap)*
                    let ret = Self::#method_name(#(#arg_names),*);
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
    pub(crate) fn class_init_override(
        &self,
        wrapper_ty: &TokenStream,
        class_ident: &syn::Ident,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        if !self.override_ {
            return None;
        }
        let arg_names = self.arg_names();
        let args_unwrap = self.args_unwrap(wrapper_ty, glib);
        let method_name = &self.handler.as_ref().unwrap().sig.ident;
        Some(quote! {
            #glib::subclass::object::ObjectClassSubclassExt::override_signal_class_handler(
                #class_ident,
                |_token, values| {
                    #(#args_unwrap)*
                    let ret = Self::#method_name(#(#arg_names),*);
                    #glib::closure::ToClosureReturnValue::to_closure_return_value(&ret)
                }
            );
        })
    }
    pub(crate) fn chain_definition(&self, glib: &TokenStream) -> Option<TokenStream> {
        if !self.override_ {
            return None;
        }
        let handler = self.handler.as_ref().unwrap();
        let output = &handler.sig.output;
        let method_name = format_ident!("parent_{}", self.name.to_snake_case());
        let arg_types = self.arg_types();
        let arg_names = arg_types.clone().map(|arg| match &*arg.pat {
            syn::Pat::Ident(syn::PatIdent { ident, .. }) => ident.clone(),
            _ => unimplemented!(),
        });
        let arg_values = arg_names.map(|arg| quote! {
            #glib::ToValue::to_value(&#arg)
        });
        let return_type = match output {
            syn::ReturnType::Type(_, ty) => quote! {
                <#ty as #glib::StaticType>::static_type()
            },
            _ => quote! { #glib::Type::UNIT },
        };
        let unwrap = match output {
            syn::ReturnType::Type(_, ty) => Some(quote! {
                <#ty as #glib::closure::TryFromClosureReturnValue>:: try_from_closure_return_value(
                    ::std::option::Option::Some(result),
                )
            }),
            _ => None,
        };
        Some(quote! {
            fn #method_name(&self, #(#arg_types),*) #output {
                let mut result = Value::from_type(#return_type);
                let values = [
                    #glib::ToValue::to_value(
                        &#glib::subclass::types::ObjectSubclassExt::instance(self)
                    ),
                    #(#arg_values),*
                ];
                unsafe {
                    #glib::gobject_ffi::g_signal_chain_from_overridden(
                        values.as_ptr() as *mut #glib::Value as *mut #glib::gobject_ffi::GValue,
                        #glib::translate::ToGlibPtrMut::to_glib_none_mut(&mut result).0,
                    );
                }
                #unwrap
            }
        })
    }
    pub(crate) fn emit_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
        if self.override_ {
            return None;
        }
        let handler = self.handler.as_ref().unwrap();
        let output = &handler.sig.output;
        let method_name = format_ident!("emit_{}", self.name.to_snake_case());
        let arg_types = self.arg_types();
        let details_arg = self
            .flags
            .contains(SignalFlags::DETAILED)
            .then(|| quote! { signal_details: ::std::option::Option<#glib::Quark>, });
        Some(quote_spanned! { handler.span() =>
            fn #method_name(&self, #details_arg #(#arg_types),*) #output
        })
    }
    pub(crate) fn emit_definition(
        &self,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        let proto = self.emit_prototype(glib)?;
        let handler = self.handler.as_ref().unwrap();
        let arg_types = self.arg_types();
        let arg_names = arg_types.clone().map(|arg| match &*arg.pat {
            syn::Pat::Ident(syn::PatIdent { ident, .. }) => ident.clone(),
            _ => unimplemented!(),
        });
        let signal_id_cell = self.signal_id_cell_ident();
        let emit = {
            let arg_names = arg_names.clone();
            quote! {
                <Self as #glib::object::ObjectExt>::emit(
                    self,
                    *#signal_id_cell,
                    &[#(&#arg_names),*]
                )
            }
        };
        let body = if self.flags.contains(SignalFlags::DETAILED) {
            quote! {
                if let Some(signal_details) = signal_details {
                    <Self as #glib::object::ObjectExt>::emit_with_details(
                        self,
                        *#signal_id_cell,
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
    pub(crate) fn connect_prototype(&self, glib: &TokenStream) -> Option<TokenStream> {
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
    pub(crate) fn connect_definition(
        &self,
        glib: &TokenStream,
    ) -> Option<TokenStream> {
        let proto = self.connect_prototype(glib)?;
        let handler = self.handler.as_ref().unwrap();
        let arg_names = self.arg_names().skip(1);
        let self_ty = quote! { Self };
        let args_unwrap = self.args_unwrap(&self_ty, glib).skip(1);

        let signal_id_cell = self.signal_id_cell_ident();
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
                    *#signal_id_cell,
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
