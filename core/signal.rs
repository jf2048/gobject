use crate::{
    util::{self, Errors},
    Concurrency, TypeBase, TypeMode,
};
use darling::{util::Flag, FromAttributes};
use heck::{ToShoutySnakeCase, ToSnakeCase};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned};
use syn::{parse_quote, spanned::Spanned};

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
    fn tokens(&self, glib: &syn::Path) -> TokenStream {
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

#[derive(Default, FromAttributes)]
#[darling(default, attributes(signal))]
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
    #[darling(rename = "override")]
    override_: Flag,
    connect: Option<bool>,
    name: Option<syn::LitStr>,
}

impl SignalAttrs {
    fn flags(&self) -> SignalFlags {
        let mut flags = SignalFlags::empty();
        flags.set(SignalFlags::RUN_FIRST, self.run_first.is_some());
        flags.set(SignalFlags::RUN_LAST, self.run_last.is_some());
        flags.set(SignalFlags::RUN_CLEANUP, self.run_cleanup.is_some());
        flags.set(SignalFlags::NO_RECURSE, self.no_recurse.is_some());
        flags.set(SignalFlags::DETAILED, self.detailed.is_some());
        flags.set(SignalFlags::ACTION, self.action.is_some());
        flags.set(SignalFlags::NO_HOOKS, self.no_hooks.is_some());
        flags.set(SignalFlags::MUST_COLLECT, self.must_collect.is_some());
        flags.set(SignalFlags::DEPRECATED, self.deprecated.is_some());
        flags
    }
}
#[derive(Default, FromAttributes)]
#[darling(default, attributes(accumulator))]
struct AccumulatorAttrs {
    signal: Option<syn::LitStr>,
}

#[derive(Debug)]
pub struct Signal {
    pub ident: syn::Ident,
    pub name: String,
    pub flags: SignalFlags,
    pub connect: bool,
    pub override_: bool,
    pub sig: Option<syn::Signature>,
    pub handler: bool,
    pub accumulator: Option<syn::Signature>,
    pub mode: TypeMode,
}

impl Signal {
    pub(crate) fn many_from_items(
        items: &mut [syn::ImplItem],
        base: TypeBase,
        mode: TypeMode,
        signals: &mut Vec<Self>,
        errors: &Errors,
    ) {
        for item in items {
            if let syn::ImplItem::Method(method) = item {
                if let Some(attrs) = util::extract_attrs(&mut method.attrs, "signal") {
                    let attr = util::parse_attributes::<SignalAttrs>(&attrs, errors);
                    let m = method.clone();
                    if method.block.stmts.is_empty() {
                        method.attrs.push(syn::parse_quote! { #[allow(dead_code)] });
                        method
                            .attrs
                            .push(syn::parse_quote! { #[allow(unused_variables)] });
                    }
                    Self::from_handler(m, attr, base, mode, signals, errors);
                } else if let Some(attrs) = util::extract_attrs(&mut method.attrs, "accumulator") {
                    let attr = util::parse_attributes::<AccumulatorAttrs>(&attrs, errors);
                    Self::from_accumulator(method.clone(), attr, mode, signals, errors);
                }
            }
        }
    }
    pub(crate) fn validate_many(signals: &[Self], errors: &Errors) {
        for signal in signals {
            if let Some(sig) = &signal.sig {
                if signal.accumulator.is_some() && matches!(sig.output, syn::ReturnType::Default) {
                    errors.push_spanned(sig, "Signal with accumulator must have return type");
                }
            } else {
                let acc = signal.accumulator.as_ref().expect("no accumulator");
                errors.push_spanned(acc, format!("No definition for signal `{}`", signal.name));
            }
            if let Some(acc) = &signal.accumulator {
                if signal.override_ {
                    errors.push_spanned(acc, "Accumulator not allowed on overriden signal");
                }
            }
        }
    }
    #[inline]
    #[allow(clippy::ptr_arg)]
    fn from_handler(
        method: syn::ImplItemMethod,
        attr: SignalAttrs,
        base: TypeBase,
        mode: TypeMode,
        signals: &mut Vec<Self>,
        errors: &Errors,
    ) {
        let ident = &method.sig.ident;
        if mode == TypeMode::Subclass && base == TypeBase::Interface {
            if let Some(recv) = method.sig.receiver() {
                errors.push_spanned(
                    recv,
                    "First argument to interface signal handler must be the wrapper type",
                );
            }
        }
        let name = attr
            .name
            .as_ref()
            .map(|n| n.value())
            .unwrap_or_else(|| util::format_name(ident));
        if !util::is_valid_name(&name) {
            errors.push_spanned(
                &name,
                format!("Invalid signal name '{}'. Signal names must start with an ASCII letter and only contain ASCII letters, numbers, '-' or '_'", name)
            );
        }
        let signal = if let Some(i) = signals.iter().position(|s| s.name == name) {
            &mut signals[i]
        } else {
            signals.push(Signal::new(ident.clone(), name.clone(), mode));
            signals.last_mut().unwrap()
        };
        if signal.sig.is_some() {
            errors.push_spanned(
                &ident,
                format!("Duplicate definition for signal `{}`", name),
            );
        }
        signal.flags = attr.flags();
        signal.connect = attr.connect.unwrap_or(true);
        signal.override_ = attr.override_.is_some();
        signal.sig = Some(method.sig);
        signal.handler = !method.block.stmts.is_empty();
        if base == TypeBase::Interface && signal.override_ {
            errors.push_spanned(&signal.ident, "`override` not allowed on interface signal");
            signal.override_ = false;
        }
    }
    #[inline]
    #[allow(clippy::ptr_arg)]
    fn from_accumulator(
        method: syn::ImplItemMethod,
        attr: AccumulatorAttrs,
        mode: TypeMode,
        signals: &mut Vec<Self>,
        errors: &Errors,
    ) {
        if !(2..=3).contains(&method.sig.inputs.len()) {
            errors.push_spanned(&method.sig.output, "Accumulator must have 2 or 3 arguments");
        }
        if let Some(recv) = method.sig.receiver() {
            errors.push_spanned(recv, "Receiver argument not allowed on accumulator");
        }
        if matches!(method.sig.output, syn::ReturnType::Default) {
            errors.push_spanned(&method.sig.output, "Accumulator must have return type");
        }
        let ident = &method.sig.ident;
        let name = attr
            .signal
            .as_ref()
            .map(|n| n.value())
            .unwrap_or_else(|| util::format_name(ident));
        let signal = if let Some(i) = signals.iter().position(|s| s.name == name) {
            &mut signals[i]
        } else {
            signals.push(Signal::new(ident.clone(), name.clone(), mode));
            signals.last_mut().unwrap()
        };
        if signal.accumulator.is_some() {
            errors.push_spanned(
                &ident,
                format!(
                    "Duplicate definition for accumulator on signal definition `{}`",
                    name
                ),
            );
        }
        signal.accumulator = Some(method.sig);
    }
    fn new(ident: syn::Ident, name: String, mode: TypeMode) -> Self {
        Self {
            ident,
            name,
            flags: SignalFlags::empty(),
            connect: false,
            override_: false,
            sig: None,
            handler: false,
            accumulator: None,
            mode,
        }
    }
    fn inputs(&self) -> impl Iterator<Item = &syn::FnArg> + Clone {
        self.sig
            .as_ref()
            .map(|s| s.inputs.iter())
            .expect("no definition for signal")
    }
    fn arg_names(&self) -> impl Iterator<Item = syn::Ident> + Clone + '_ {
        self.inputs()
            .enumerate()
            .map(|(i, _)| format_ident!("arg{}", i, span = Span::mixed_site()))
    }
    fn args_unwrap<'a>(
        &'a self,
        args_ident: &'a syn::Ident,
        self_ty: &'a TokenStream,
        glib: &'a syn::Path,
    ) -> impl Iterator<Item = TokenStream> + 'a {
        let recv = self.sig.as_ref().and_then(|s| s.receiver()).map(|recv| {
            let arg_name = syn::Ident::new("arg0", Span::mixed_site());
            let ty = match recv {
                syn::FnArg::Receiver(_) => parse_quote! { #self_ty },
                syn::FnArg::Typed(t) => t.ty.as_ref().clone(),
            };
            let unwrap_recv = (self.mode == TypeMode::Subclass).then(|| {
                let ref_ = (!matches!(ty, syn::Type::Reference(_))).then(|| quote! { & });
                quote_spanned! { recv.span() =>
                    let #arg_name = #glib::subclass::prelude::ObjectSubclassIsExt::imp(#ref_ #arg_name);
                }
            });
            quote_spanned! { recv.span() =>
                let #arg_name = #args_ident[0usize].get::<#ty>().unwrap_or_else(|e| {
                    ::std::panic!(
                        "Wrong type for argument {}: {:?}",
                        0usize,
                        e
                    )
                });
                #unwrap_recv
            }
        });
        let offset = recv.as_ref().map(|_| 1).unwrap_or(0);
        let rest = self
            .inputs()
            .enumerate()
            .skip(offset)
            .map(move |(index, input)| {
                let ty = match input {
                    syn::FnArg::Typed(t) => {
                        let ty = &t.ty;
                        quote_spanned! { ty.span() => #ty }
                    }
                    syn::FnArg::Receiver(_) => unreachable!(),
                };
                let arg_name = format_ident!("arg{}", index, span = Span::mixed_site());
                let error_ident = syn::Ident::new("e", Span::mixed_site());
                quote_spanned! { input.span() =>
                    let #arg_name = #args_ident[#index].get::<#ty>().unwrap_or_else(|#error_ident| {
                        ::std::panic!(
                            "Wrong type for argument {}: {:?}",
                            #index,
                            #error_ident
                        )
                    });
                }
            });
        recv.into_iter().chain(rest)
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
                    ident: format_ident!("arg{}", index, span = Span::mixed_site()),
                    subpat: None,
                }));
            }
            ty
        })
    }
    fn signal_id_cell_ident(&self) -> syn::Ident {
        format_ident!(
            "SIGNAL_{}",
            self.name.to_shouty_snake_case(),
            span = Span::mixed_site()
        )
    }
    pub(crate) fn signal_id_cell_definition(
        &self,
        wrapper_ty: &TokenStream,
        glib: &syn::Path,
    ) -> TokenStream {
        let name = &self.name;
        let ident = self.signal_id_cell_ident();
        quote! {
            #[doc(hidden)]
            static #ident: #glib::once_cell::sync::Lazy<#glib::subclass::SignalId> =
                #glib::once_cell::sync::Lazy::new(|| {
                    #glib::subclass::SignalId::lookup(
                        #name,
                        <#wrapper_ty as #glib::StaticType>::static_type(),
                    ).unwrap_or_else(|| {
                        ::std::panic!(
                            "Signal `{}` not registered",
                            #name
                        )
                    })
                });
        }
    }
    pub(crate) fn definition(
        &self,
        wrapper_ty: &TokenStream,
        sub_ty: &TokenStream,
        glib: &syn::Path,
    ) -> Option<TokenStream> {
        if self.override_ {
            return None;
        }

        let Self {
            name,
            flags,
            sig,
            accumulator,
            ..
        } = self;

        let sig = sig.as_ref()?;
        let inputs = self.inputs();
        let input_static_types = inputs.skip(1).map(|input| {
            let ty = match &input {
                syn::FnArg::Typed(t) => &t.ty,
                _ => unimplemented!(),
            };
            quote_spanned! { ty.span() =>
                <#glib::subclass::SignalType as ::core::convert::From<#glib::Type>>::from(
                    <#ty as #glib::types::StaticType>::static_type()
                )
            }
        });
        let dest = match self.mode {
            TypeMode::Subclass => sub_ty,
            TypeMode::Wrapper => wrapper_ty,
        };
        let builder = syn::Ident::new("builder", Span::mixed_site());
        let class_handler = self.handler.then(|| {
            let arg_names = self.arg_names();
            let token_ident = syn::Ident::new("_token", Span::mixed_site());
            let args_ident = syn::Ident::new("args", Span::mixed_site());
            let ret_ident = syn::Ident::new("ret", Span::mixed_site());
            let args_unwrap = self.args_unwrap(&args_ident, wrapper_ty, glib);
            let method_name = &sig.ident;
            let handler_name =
                format_ident!("{}_class_handler", method_name, span = method_name.span());
            quote_spanned! { sig.span() =>
                #[inline]
                fn #handler_name(
                    #token_ident: &#glib::subclass::SignalClassHandlerToken,
                    #args_ident: &[#glib::Value]
                ) -> ::std::option::Option<#glib::Value> {
                    #(#args_unwrap)*
                    let #ret_ident = #dest::#method_name(#(#arg_names),*);
                    #glib::closure::ToClosureReturnValue::to_closure_return_value(&#ret_ident)
                }
                let #builder = #builder.class_handler(#handler_name);
            }
        });
        let output = match &sig.output {
            syn::ReturnType::Type(_, ty) => quote! { #ty },
            _ => quote! { () },
        };
        let accumulator = accumulator.as_ref().map(|sig| {
            let method_name = &sig.ident;
            let acc_name = format_ident!("{}_accumulator", method_name, span = method_name.span());
            let hint = syn::Ident::new("_hint", Span::mixed_site());
            let accu = syn::Ident::new("accu", Span::mixed_site());
            let cur_accu = syn::Ident::new("cur_accu", Span::mixed_site());
            let value = syn::Ident::new("value", Span::mixed_site());
            let ret = syn::Ident::new("ret", Span::mixed_site());
            let next = syn::Ident::new("next", Span::mixed_site());
            let call_args = if sig.inputs.len() == 2 {
                quote! { #cur_accu, #value }
            } else {
                quote! { #hint, #cur_accu, #value }
            };
            quote_spanned! { sig.span() =>
                #[inline]
                fn #acc_name(
                    #hint: &#glib::subclass::SignalInvocationHint,
                    #accu: &mut #glib::Value,
                    #value: &#glib::Value
                ) -> ::std::primitive::bool {
                    let #cur_accu = #accu.get().unwrap();
                    let #value = #value.get().unwrap();
                    let (#next, #ret) = match #dest::#method_name(#call_args) {
                        ::std::ops::ControlFlow::Continue(#next) => (#next, true),
                        ::std::ops::ControlFlow::Break(#next) => (#next, false),
                    };
                    if let ::std::option::Option::Some(#next) = #next {
                        *#accu = #glib::ToValue::to_value(&#next);
                    }
                    #ret
                }
                let #builder = #builder.accumulator(#acc_name);
            }
        });
        let flags = (!flags.is_empty()).then(|| {
            let flags = flags.tokens(glib);
            quote! { let #builder = #builder.flags(#flags); }
        });
        let param_types = syn::Ident::new("param_types", Span::mixed_site());
        Some(quote_spanned! { sig.span() =>
            {
                let #param_types = [#(#input_static_types),*];
                let #builder = #glib::subclass::Signal::#builder(
                    #name,
                    &#param_types,
                    <#glib::subclass::SignalType as ::core::convert::From<#glib::Type>>::from(
                        <#output as #glib::types::StaticType>::static_type()
                    ),
                );
                #flags
                #class_handler
                #accumulator
                #builder.build()
            }
        })
    }
    pub(crate) fn class_init_override(
        &self,
        wrapper_ty: &TokenStream,
        sub_ty: &TokenStream,
        class_ident: &TokenStream,
        glib: &syn::Path,
    ) -> Option<TokenStream> {
        if !self.override_ {
            return None;
        }
        let arg_names = self.arg_names();
        let token_ident = syn::Ident::new("_token", Span::mixed_site());
        let args_ident = syn::Ident::new("args", Span::mixed_site());
        let ret_ident = syn::Ident::new("ret", Span::mixed_site());
        let args_unwrap = self.args_unwrap(&args_ident, wrapper_ty, glib);
        let dest = match self.mode {
            TypeMode::Subclass => sub_ty,
            TypeMode::Wrapper => wrapper_ty,
        };
        let name = &self.name;
        let sig = self.sig.as_ref()?;
        let method_name = &sig.ident;
        let override_ident = format_ident!(
            "{}_override_handler",
            method_name,
            span = method_name.span()
        );
        Some(quote_spanned! { sig.span() => {
            #[inline]
            fn #override_ident(
                #token_ident: &#glib::subclass::SignalClassHandlerToken,
                #args_ident: &[#glib::Value]
            ) -> ::std::option::Option<#glib::Value> {
                #(#args_unwrap)*
                let #ret_ident = #dest::#method_name(#(#arg_names),*);
                #glib::closure::ToClosureReturnValue::to_closure_return_value(&#ret_ident)
            }
            #glib::subclass::object::ObjectClassSubclassExt::override_signal_class_handler(
                #class_ident,
                #name,
                #override_ident,
            );
        }})
    }
    pub(crate) fn chain_definition(&self, mode: TypeMode, glib: &syn::Path) -> Option<TokenStream> {
        if !self.override_ {
            return None;
        }
        if mode != self.mode {
            return None;
        }
        let sig = self.sig.as_ref()?;
        let output = &sig.output;
        let name = &self.name;
        let method_name = format_ident!(
            "parent_{}",
            self.name.to_snake_case(),
            span = sig.ident.span()
        );
        let arg_types = self.arg_types();
        let arg_names = arg_types.clone().map(|arg| match &*arg.pat {
            syn::Pat::Ident(syn::PatIdent { ident, .. }) => ident.clone(),
            _ => unimplemented!(),
        });
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let unwrap_recv = match self.mode {
            TypeMode::Subclass => quote_spanned! { sig.span() =>
                #glib::subclass::types::ObjectSubclassExt::instance(#self_ident)
            },
            TypeMode::Wrapper => quote! { #self_ident },
        };
        let arg_values = arg_names.map(|arg| {
            quote_spanned! { arg.span() =>
                #glib::ToValue::to_value(&#arg)
            }
        });
        let result_ident = syn::Ident::new("result", Span::mixed_site());
        let values_ident = syn::Ident::new("values", Span::mixed_site());
        let declare_result = match output {
            syn::ReturnType::Type(_, ty) => Some(quote_spanned! { ty.span() =>
                let mut #result_ident = #glib::Value::from_type(
                    <#ty as #glib::StaticType>::static_type()
                );
            }),
            syn::ReturnType::Default => None,
        };
        let result_ptr = match output {
            syn::ReturnType::Type(_, ty) => quote_spanned! { ty.span() =>
                #glib::translate::ToGlibPtrMut::to_glib_none_mut(&mut #result_ident).0
            },
            syn::ReturnType::Default => quote! { ::std::ptr::null_mut() },
        };
        let unwrap = match output {
            syn::ReturnType::Type(_, ty) => {
                let error_ident = syn::Ident::new("e", Span::mixed_site());
                Some(quote_spanned! { ty.span() =>
                    <#ty as #glib::closure::TryFromClosureReturnValue>:: try_from_closure_return_value(
                        ::std::option::Option::Some(#result_ident),
                    ).unwrap_or_else(|#error_ident| {
                        ::std::panic!(
                            "Invalid return type from chained signal handler for `{}`: {}",
                            #name,
                            #error_ident,
                        )
                    })
                })
            }
            syn::ReturnType::Default => None,
        };
        Some(quote_spanned! { sig.span() =>
            fn #method_name(&#self_ident, #(#arg_types),*) #output {
                #declare_result
                let #values_ident = [
                    #glib::ToValue::to_value(&#unwrap_recv),
                    #(#arg_values),*
                ];
                unsafe {
                    #glib::gobject_ffi::g_signal_chain_from_overridden(
                        #values_ident.as_ptr() as *mut #glib::Value as *mut #glib::gobject_ffi::GValue,
                        #result_ptr,
                    );
                }
                #unwrap
            }
        })
    }
    fn emit_prototype(&self, glib: &syn::Path) -> Option<TokenStream> {
        if self.override_ {
            return None;
        }
        let sig = self.sig.as_ref()?;
        let output = &sig.output;
        let method_name = format_ident!(
            "emit_{}",
            self.name.to_snake_case(),
            span = sig.ident.span()
        );
        let arg_types = self.arg_types();
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let details_ident = syn::Ident::new("signal_details", Span::mixed_site());
        let details_arg = self
            .flags
            .contains(SignalFlags::DETAILED)
            .then(|| quote! { #details_ident: ::std::option::Option<#glib::Quark>, });
        Some(quote_spanned! { sig.span() =>
            fn #method_name(&#self_ident, #details_arg #(#arg_types),*) #output
        })
    }
    fn emit_definition(&self, glib: &syn::Path) -> Option<TokenStream> {
        let proto = self.emit_prototype(glib)?;
        let sig = self.sig.as_ref()?;
        let arg_types = self.arg_types();
        let arg_names = arg_types.clone().map(|arg| match &*arg.pat {
            syn::Pat::Ident(syn::PatIdent { ident, .. }) => ident.clone(),
            _ => unimplemented!(),
        });
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let details_ident = syn::Ident::new("signal_details", Span::mixed_site());
        let signal_id_cell = self.signal_id_cell_ident();
        let emit = {
            let arg_names = arg_names.clone();
            quote! {
                <Self as #glib::object::ObjectExt>::emit(
                    #self_ident,
                    *#signal_id_cell,
                    &[#(&#arg_names),*]
                )
            }
        };
        let body = if self.flags.contains(SignalFlags::DETAILED) {
            quote! {
                if let Some(#details_ident) = #details_ident {
                    <Self as #glib::object::ObjectExt>::emit_with_details(
                        #self_ident,
                        *#signal_id_cell,
                        #details_ident,
                        &[#(&#arg_names),*]
                    )
                } else {
                    #emit
                }
            }
        } else {
            emit
        };
        Some(quote_spanned! { sig.span() =>
            #proto {
                #![inline]
                #body
            }
        })
    }
    fn connect_prototype(
        &self,
        concurrency: Concurrency,
        local: bool,
        glib: &syn::Path,
    ) -> Option<TokenStream> {
        if !self.connect || self.override_ {
            return None;
        }
        let sig = self.sig.as_ref()?;
        let method_name = if local {
            format_ident!(
                "connect_{}_local",
                self.name.to_snake_case(),
                span = sig.ident.span()
            )
        } else {
            format_ident!(
                "connect_{}",
                self.name.to_snake_case(),
                span = sig.ident.span()
            )
        };
        let output = &sig.output;
        let input_types = self.inputs().skip(1).map(|arg| match arg {
            syn::FnArg::Typed(t) => &t.ty,
            _ => unimplemented!(),
        });
        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let func_ident = syn::Ident::new("func", Span::mixed_site());
        let details_arg = self
            .flags
            .contains(SignalFlags::DETAILED)
            .then(|| quote_spanned! { Span::mixed_site() => details: ::std::option::Option<#glib::Quark>, });
        Some(quote_spanned! { sig.span() =>
            fn #method_name<____Func: Fn(&Self, #(#input_types),*) #output #concurrency + 'static>(
                &#self_ident,
                #details_arg
                #func_ident: ____Func,
            ) -> #glib::SignalHandlerId
        })
    }
    fn connect_definition(
        &self,
        concurrency: Concurrency,
        local: bool,
        glib: &syn::Path,
    ) -> Option<TokenStream> {
        let proto = self.connect_prototype(concurrency, local, glib)?;
        let sig = self.sig.as_ref()?;
        let arg_names = self.arg_names().skip(1);
        let self_ty = quote! { Self };

        let self_ident = syn::Ident::new("self", Span::mixed_site());
        let func_ident = syn::Ident::new("func", Span::mixed_site());
        let args_ident = syn::Ident::new("args", Span::mixed_site());
        let recv_ident = syn::Ident::new("recv", Span::mixed_site());
        let ret_ident = syn::Ident::new("_ret", Span::mixed_site());
        let args_unwrap = self.args_unwrap(&args_ident, &self_ty, glib).skip(1);

        let signal_id_cell = self.signal_id_cell_ident();
        let details = if self.flags.contains(SignalFlags::DETAILED) {
            quote_spanned! { Span::mixed_site() => details }
        } else {
            quote! { ::std::option::Option::None }
        };
        let call = if concurrency == Concurrency::None {
            format_ident!("connect_local_id")
        } else {
            format_ident!("connect_id")
        };

        let unwrap = match &sig.output {
            syn::ReturnType::Type(_, _) => quote! {
                #glib::closure::ToClosureReturnValue::to_closure_return_value(&#ret_ident)
            },
            _ => quote! { ::core::option::Option::None },
        };
        Some(quote_spanned! { sig.span() =>
            #proto {
                #![inline]
                <Self as #glib::object::ObjectExt>::#call(
                    #self_ident,
                    *#signal_id_cell,
                    #details,
                    false,
                    move |#args_ident| {
                        let #recv_ident = #args_ident[0].get::<Self>().unwrap();
                        #(#args_unwrap)*
                        let #ret_ident = #func_ident(&#recv_ident, #(#arg_names),*);
                        #unwrap
                    },
                )
            }
        })
    }
    pub(crate) fn method_prototypes(
        &self,
        concurrency: Concurrency,
        glib: &syn::Path,
    ) -> Vec<TokenStream> {
        [
            self.emit_prototype(glib),
            self.connect_prototype(concurrency, false, glib),
            (concurrency != Concurrency::None)
                .then(|| self.connect_prototype(Concurrency::None, true, glib))
                .flatten(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
    pub(crate) fn method_definitions(
        &self,
        concurrency: Concurrency,
        glib: &syn::Path,
    ) -> Vec<TokenStream> {
        [
            self.emit_definition(glib),
            self.connect_definition(concurrency, false, glib),
            (concurrency != Concurrency::None)
                .then(|| self.connect_definition(Concurrency::None, true, glib))
                .flatten(),
        ]
        .into_iter()
        .flatten()
        .collect()
    }
}
