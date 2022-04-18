use crate::util::Errors;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use std::collections::HashSet;
use std::rc::Rc;
use syn::{parse::Parser, parse_quote, parse_quote_spanned, spanned::Spanned, visit_mut::VisitMut};

enum Capture {
    Strong {
        ident: Option<syn::Ident>,
        from: Option<syn::Expr>,
    },
    Weak {
        ident: Option<syn::Ident>,
        from: Option<syn::Expr>,
        or: Option<Rc<UpgradeFailAction>>,
    },
    Watch {
        ident: Option<syn::Ident>,
        from: Option<syn::Expr>,
    },
}

#[derive(Clone)]
enum UpgradeFailAction {
    AllowNone,
    Panic,
    Default(syn::Expr),
    Return(Option<syn::Expr>),
}

#[derive(Eq, PartialEq, Ord, PartialOrd, Copy, Clone)]
enum Mode {
    Clone,
    Closure,
    ClosureAsync,
}

fn is_simple_expr(mut expr: &syn::Expr) -> bool {
    loop {
        match expr {
            syn::Expr::Cast(e) => expr = &*e.expr,
            syn::Expr::Field(e) => expr = &*e.base,
            syn::Expr::Index(e) => return is_simple_expr(&*e.expr) && is_simple_expr(&*e.index),
            syn::Expr::Lit(_) => return true,
            syn::Expr::Paren(e) => expr = &*e.expr,
            syn::Expr::Path(_) => return true,
            syn::Expr::Reference(e) => expr = &*e.expr,
            syn::Expr::Type(e) => expr = &*e.expr,
            _ => return false,
        }
    }
}

impl Capture {
    fn set_default_fail(&mut self, action: &Rc<UpgradeFailAction>) {
        if let Self::Weak { or, .. } = self {
            if or.is_none() {
                *or = Some(action.clone());
            }
        }
    }
    fn outer_tokens(&self, index: usize, go: &syn::Ident) -> Option<TokenStream> {
        Some(match self {
            Self::Strong { ident, from } => {
                let target = format_ident!("____strong{}", index, span = Span::mixed_site());
                let input = from
                    .as_ref()
                    .map(|f| f.to_token_stream())
                    .or_else(|| Some(ident.as_ref()?.to_token_stream()))?;
                quote! { let #target = ::std::clone::Clone::clone(&#input); }
            }
            Self::Weak { ident, from, .. } => {
                let target = format_ident!("____weak{}", index, span = Span::mixed_site());
                let input = from
                    .as_ref()
                    .map(|f| f.to_token_stream())
                    .or_else(|| Some(ident.as_ref()?.to_token_stream()))?;
                quote! { let #target = #go::glib::clone::Downgrade::downgrade(&#input); }
            }
            Self::Watch { ident, from } => {
                let target = format_ident!("____watch{}", index, span = Span::mixed_site());
                let input = from
                    .as_ref()
                    .map(|f| f.to_token_stream())
                    .or_else(|| Some(ident.as_ref()?.to_token_stream()))?;
                if from.as_ref().map(is_simple_expr).unwrap_or(true) {
                    quote! {
                        let #target = #go::glib::object::Watchable::watched_object(&#input);
                    }
                } else {
                    let watch_ident = syn::Ident::new("____watch", Span::mixed_site());
                    quote! {
                        let #watch_ident = ::std::clone::Clone::clone(&#input);
                        let #target = #go::glib::object::Watchable::watched_object(&#watch_ident);
                    }
                }
            }
        })
    }
    fn rename_tokens(&self, index: usize) -> Option<TokenStream> {
        Some(match self {
            Self::Strong { ident, .. } => {
                let ident = ident.as_ref()?;
                let input = format_ident!("____strong{}", index, span = Span::mixed_site());
                quote! { let #ident = #input; }
            }
            _ => return None,
        })
    }
    fn inner_tokens(&self, index: usize, mode: Mode, go: &syn::Ident) -> Option<TokenStream> {
        Some(match self {
            Self::Strong { .. } => return None,
            Self::Weak { ident, or, .. } => {
                let ident = ident.as_ref()?;
                let input = format_ident!("____weak{}", index, span = Span::mixed_site());
                let upgrade = quote! { #go::glib::clone::Upgrade::upgrade(&#input) };
                let upgrade = match or.as_ref().map(|or| or.as_ref()) {
                    None | Some(UpgradeFailAction::AllowNone) => upgrade,
                    Some(or) => {
                        let action = match or {
                            UpgradeFailAction::Panic => {
                                let name = ident.to_string();
                                quote! { ::std::panic!("Failed to upgrade `{}`", #name) }
                            }
                            UpgradeFailAction::Default(expr) => expr.to_token_stream(),
                            UpgradeFailAction::Return(expr) => {
                                if mode != Mode::Clone {
                                    quote! {
                                        return #go::glib::closure::ToClosureReturnValue::to_closure_return_value(
                                            &#expr
                                        )
                                    }
                                } else {
                                    quote! { return #expr }
                                }
                            }
                            UpgradeFailAction::AllowNone => unreachable!(),
                        };
                        quote_spanned! { Span::mixed_site() =>
                            match #upgrade {
                                ::std::option::Option::Some(v) => v,
                                ::std::option::Option::None => #action
                            }
                        }
                    }
                };
                quote! { let #ident = #upgrade;  }
            }
            Self::Watch { ident, .. } => {
                if mode == Mode::ClosureAsync {
                    return None;
                }
                let ident = ident.as_ref()?;
                let input = format_ident!("____watch{}", index, span = Span::mixed_site());
                quote! {
                    let #ident = unsafe { #input.borrow() };
                    let #ident = ::std::convert::AsRef::as_ref(&#ident);
                }
            }
        })
    }
    fn async_inner_tokens(&self, index: usize) -> Option<TokenStream> {
        Some(match self {
            Self::Strong { ident, .. } => {
                ident.as_ref()?;
                quote! { let #ident = ::std::clone::Clone::clone(&#ident); }
            }
            Self::Weak { ident, .. } => {
                ident.as_ref()?;
                let input = format_ident!("____weak{}", index, span = Span::mixed_site());
                quote! { let #input = ::std::clone::Clone::clone(&#input); }
            }
            Self::Watch { ident, .. } => {
                let ident = ident.as_ref()?;
                let input = format_ident!("____watch{}", index, span = Span::mixed_site());
                quote! {
                    let #ident = ::std::clone::Clone::clone(unsafe { &*#input.borrow() });
                }
            }
        })
    }
    fn after_tokens(&self, go: &syn::Ident) -> Option<TokenStream> {
        Some(match self {
            Self::Watch { ident, from } if ident.is_some() || from.is_some() => {
                let closure_ident = syn::Ident::new("____closure", Span::mixed_site());
                if from.as_ref().map(is_simple_expr).unwrap_or(true) {
                    let input = from
                        .as_ref()
                        .map(|f| f.to_token_stream())
                        .or_else(|| Some(ident.as_ref()?.to_token_stream()))?;
                    quote! {
                        #go::glib::object::Watchable::watch_closure(&#input, &#closure_ident);
                    }
                } else {
                    let watch_ident = syn::Ident::new("____watch", Span::mixed_site());
                    quote! {
                        #go::glib::object::ObjectExt::watch_closure(&#watch_ident, &#closure_ident);
                    }
                }
            }
            _ => return None,
        })
    }
}

fn extract_idents(pat: &syn::Pat, idents: &mut HashSet<syn::Ident>) {
    use syn::Pat::*;
    match pat {
        Box(p) => extract_idents(&*p.pat, idents),
        Ident(p) => {
            idents.insert(p.ident.clone());
        }
        Or(p) => p.cases.iter().for_each(|p| extract_idents(p, idents)),
        Reference(p) => extract_idents(&*p.pat, idents),
        Slice(p) => p.elems.iter().for_each(|p| extract_idents(p, idents)),
        Struct(p) => p
            .fields
            .iter()
            .for_each(|p| extract_idents(&*p.pat, idents)),
        Tuple(p) => p.elems.iter().for_each(|p| extract_idents(p, idents)),
        TupleStruct(p) => p.pat.elems.iter().for_each(|p| extract_idents(p, idents)),
        Type(p) => extract_idents(&*p.pat, idents),
        _ => {}
    }
}

mod keywords {
    syn::custom_keyword!(local);
    syn::custom_keyword!(or);
    syn::custom_keyword!(or_panic);
    syn::custom_keyword!(or_return);
    syn::custom_keyword!(allow_none);
}

fn parse_closure(input: syn::parse::ParseStream<'_>) -> syn::Result<bool> {
    if input.is_empty() {
        return Ok(false);
    }
    let content;
    syn::parenthesized!(content in input);
    if content.is_empty() {
        return Ok(false);
    }
    let lookahead = content.lookahead1();
    let local = if content.is_empty() {
        false
    } else if lookahead.peek(keywords::local) {
        content.parse::<keywords::local>()?;
        true
    } else {
        return Err(lookahead.error());
    };
    content.parse::<syn::parse::Nothing>()?;
    Ok(local)
}

fn parse_strong(input: syn::parse::ParseStream<'_>) -> syn::Result<Option<syn::Expr>> {
    if input.is_empty() {
        return Ok(None);
    }
    let content;
    syn::parenthesized!(content in input);
    if content.is_empty() {
        return Ok(None);
    }
    let expr = content.parse()?;
    content.parse::<syn::parse::Nothing>()?;
    Ok(Some(expr))
}

#[inline]
fn has_expr(input: syn::parse::ParseBuffer) -> bool {
    // check if only one token
    if input.peek(keywords::or_panic) || input.peek(keywords::allow_none) {
        input.parse::<syn::Ident>().unwrap();
        if input.is_empty() {
            return false;
        }
    }
    // check if only one token and one expr
    if input.peek(keywords::or) || input.peek(keywords::or_return) {
        input.parse::<syn::Ident>().unwrap();
        if input.is_empty() {
            return false;
        }
        if input.parse::<syn::Expr>().is_err() {
            return false;
        }
        if input.is_empty() {
            return false;
        }
    }
    true
}

fn parse_weak(
    input: syn::parse::ParseStream<'_>,
) -> syn::Result<(Option<syn::Expr>, Option<UpgradeFailAction>)> {
    if input.is_empty() {
        return Ok((None, None));
    }
    let content;
    syn::parenthesized!(content in input);
    if content.is_empty() {
        return Ok((None, None));
    }
    let expr = if has_expr(content.fork()) {
        Some(content.parse()?)
    } else {
        None
    };
    let lookahead = content.lookahead1();
    let fail_action = if lookahead.peek(keywords::or) {
        content.parse::<keywords::or>()?;
        let ret = content.parse()?;
        Some(UpgradeFailAction::Default(ret))
    } else if lookahead.peek(keywords::or_panic) {
        content.parse::<keywords::or_panic>()?;
        Some(UpgradeFailAction::Panic)
    } else if lookahead.peek(keywords::or_return) {
        content.parse::<keywords::or_return>()?;
        let ret = if content.is_empty() {
            None
        } else {
            Some(content.parse()?)
        };
        Some(UpgradeFailAction::Return(ret))
    } else if lookahead.peek(keywords::allow_none) {
        content.parse::<keywords::allow_none>()?;
        Some(UpgradeFailAction::AllowNone)
    } else if content.is_empty() {
        None
    } else {
        return Err(lookahead.error());
    };
    content.parse::<syn::parse::Nothing>()?;
    Ok((expr, fail_action))
}

macro_rules! pat_attrs {
    ($pat:ident) => {{
        use syn::*;
        Some(match $pat {
            Pat::Box(PatBox { attrs, .. }) => attrs,
            Pat::Ident(PatIdent { attrs, .. }) => attrs,
            Pat::Lit(PatLit { attrs, .. }) => attrs,
            Pat::Macro(PatMacro { attrs, .. }) => attrs,
            Pat::Or(PatOr { attrs, .. }) => attrs,
            Pat::Path(PatPath { attrs, .. }) => attrs,
            Pat::Range(PatRange { attrs, .. }) => attrs,
            Pat::Reference(PatReference { attrs, .. }) => attrs,
            Pat::Rest(PatRest { attrs, .. }) => attrs,
            Pat::Slice(PatSlice { attrs, .. }) => attrs,
            Pat::Struct(PatStruct { attrs, .. }) => attrs,
            Pat::Tuple(PatTuple { attrs, .. }) => attrs,
            Pat::TupleStruct(PatTupleStruct { attrs, .. }) => attrs,
            Pat::Type(PatType { attrs, .. }) => attrs,
            Pat::Wild(PatWild { attrs, .. }) => attrs,
            _ => return None,
        })
    }};
}

fn pat_attrs(pat: &syn::Pat) -> Option<&Vec<syn::Attribute>> {
    pat_attrs!(pat)
}

fn pat_attrs_mut(pat: &mut syn::Pat) -> Option<&mut Vec<syn::Attribute>> {
    pat_attrs!(pat)
}

fn has_captures<'p>(mut inputs: impl Iterator<Item = &'p syn::Pat>) -> bool {
    inputs.any(|pat| {
        pat_attrs(pat).iter().any(|attrs| {
            attrs
                .iter()
                .any(|a| a.path.is_ident("strong") || a.path.is_ident("weak"))
        })
    })
}

struct Visitor<'v> {
    crate_ident: &'v syn::Ident,
    errors: &'v Errors,
}

impl<'v> Visitor<'v> {
    fn create_gclosure(&mut self, closure: &syn::ExprClosure) -> Option<syn::Expr> {
        let closure_index = closure
            .attrs
            .iter()
            .position(|a| a.path.is_ident("closure"));
        let has_watch = closure.inputs.iter().any(|pat| {
            pat_attrs(pat)
                .iter()
                .any(|attrs| attrs.iter().any(|a| a.path.is_ident("watch")))
        });
        if closure_index.is_none() && !has_watch {
            return None;
        }
        let has_captures = has_captures(closure.inputs.iter());
        if (has_captures || has_watch) && closure.capture.is_none() {
            self.errors.push_spanned(
                closure,
                "Closure must be `move` to use #[watch] or #[strong] or #[weak]",
            );
        }

        let mut attrs = closure.attrs.clone();
        let local = if let Some(closure_index) = closure_index {
            let attr = attrs.remove(closure_index);
            parse_closure.parse2(attr.tokens).unwrap_or_else(|e| {
                self.errors.push_syn(e);
                false
            })
        } else {
            true
        };

        let mode = match closure.body.as_ref() {
            syn::Expr::Async(_) => Mode::ClosureAsync,
            _ => Mode::Closure,
        };
        let mut inputs = closure.inputs.iter().cloned().collect::<Vec<_>>();
        let mut rest_index = None;
        let mut captures = (has_captures || has_watch)
            .then(|| self.get_captures(&mut inputs, mode))
            .flatten()
            .unwrap_or_default();
        if let Some(action) = self.get_default_fail_action(&mut attrs) {
            let action = Rc::new(action);
            for capture in &mut captures {
                capture.set_default_fail(&action);
            }
        }

        for (index, pat) in inputs.iter_mut().enumerate() {
            if let Some(attrs) = pat_attrs_mut(pat) {
                let attr_index = attrs.iter().position(|a| a.path.is_ident("rest"));
                if let Some(attr_index) = attr_index {
                    let attr = attrs.remove(attr_index);
                    if !attr.tokens.is_empty() {
                        self.errors
                            .push_spanned(&attr.tokens, "Unknown tokens on rest parameter");
                    }
                    rest_index = Some(index);
                    break;
                }
            }
        }
        if let Some(rest_index) = rest_index {
            while inputs.len() > rest_index + 1 {
                let pat = inputs.remove(rest_index + 1);
                self.errors
                    .push_spanned(pat, "Arguments not allowed past #[rest] parameter");
            }
        }

        let go = self.crate_ident;
        let closure_ident = syn::Ident::new("____closure", Span::mixed_site());
        let values_ident = syn::Ident::new("____values", Span::mixed_site());
        let constructor = if local {
            format_ident!("new_local")
        } else {
            format_ident!("new")
        };
        let outer = captures
            .iter()
            .enumerate()
            .map(|(i, c)| c.outer_tokens(i, go));
        let rename = captures.iter().enumerate().map(|(i, c)| c.rename_tokens(i));
        let inner = captures
            .iter()
            .enumerate()
            .map(|(i, c)| c.inner_tokens(i, mode, go));
        let after = captures.iter().map(|c| c.after_tokens(go));
        let args_len = inputs.len() - rest_index.map(|_| 1).unwrap_or(0);
        let arg_unwraps = inputs.iter().enumerate().map(|(index, pat)| match pat {
            syn::Pat::Wild(_) => None,
            _ => {
                let attrs = pat_attrs(pat).into_iter().flat_map(|a| a.iter());
                Some(if Some(index) == rest_index {
                    quote! {
                        #(#attrs)*
                        let #pat = &#values_ident[#index..#values_ident.len()];
                    }
                } else {
                    quote! {
                        #(#attrs)*
                        let #pat = #go::glib::Value::get(&#values_ident[#index])
                            .unwrap_or_else(|e| {
                                ::std::panic!("Wrong type for closure argument {}: {:?}", #index, e)
                            });
                    }
                })
            }
        });
        let expr = &closure.body;
        let inner_body = quote! { {
            if #values_ident.len() < #args_len {
                ::std::panic!(
                    "Closure called with wrong number of arguments: Expected {}, got {}",
                    #args_len,
                    #values_ident.len(),
                );
            }
            #(#inner)*
            #(#arg_unwraps)*
            #expr
        } };
        let body = if mode == Mode::ClosureAsync {
            let async_inner = captures
                .iter()
                .enumerate()
                .map(|(i, c)| c.async_inner_tokens(i));
            quote! {
                let #values_ident = #values_ident.to_vec();
                #(#async_inner)*
                #go::glib::MainContext::default().spawn_local(
                    async move { let _: () = #inner_body.await; }
                );
                ::std::option::Option::None
            }
        } else {
            let inner_body = match &closure.output {
                syn::ReturnType::Type(_, ty) => {
                    let ret = syn::Ident::new("____ret", Span::mixed_site());
                    quote! {
                        {
                            let #ret: #ty = #inner_body;
                            #ret
                        }
                    }
                }
                _ => quote! { #inner_body },
            };
            quote! {
                #go::glib::closure::ToClosureReturnValue::to_closure_return_value(
                    &#inner_body
                )
            }
        };
        Some(parse_quote_spanned! { Span::mixed_site() =>
            {
                #(#outer)*
                #(#rename)*
                let #closure_ident = #go::glib::closure::RustClosure::#constructor(move |#values_ident| {
                    #body
                });
                #(#after)*
                #closure_ident
            }
        })
    }

    fn create_closure(&mut self, closure: &syn::ExprClosure) -> Option<syn::Expr> {
        if !has_captures(closure.inputs.iter()) {
            return None;
        }
        if closure.capture.is_none() {
            self.errors.push_spanned(
                closure,
                "Closure must be `move` to use #[strong] or #[weak]",
            );
        }
        let mut inputs = closure.inputs.iter().cloned().collect::<Vec<_>>();
        self.get_captures(&mut inputs, Mode::Clone)
            .map(|mut captures| {
                let mut body = closure.clone();
                if let Some(action) = self.get_default_fail_action(&mut body.attrs) {
                    let action = Rc::new(action);
                    for capture in &mut captures {
                        capture.set_default_fail(&action);
                    }
                }
                let go = self.crate_ident;
                let outer = captures
                    .iter()
                    .enumerate()
                    .map(|(i, c)| c.outer_tokens(i, go));
                let rename = captures.iter().enumerate().map(|(i, c)| c.rename_tokens(i));
                let inner = captures
                    .iter()
                    .enumerate()
                    .map(|(i, c)| c.inner_tokens(i, Mode::Clone, go));
                body.inputs = FromIterator::from_iter(inputs.into_iter());
                if matches!(body.body.as_ref(), syn::Expr::Async(_)) {
                    body.body = Box::new({
                        let syn::ExprAsync {
                            attrs,
                            capture,
                            block,
                            ..
                        } = match body.body.as_ref() {
                            syn::Expr::Async(e) => e,
                            _ => unreachable!(),
                        };
                        parse_quote! {
                            #(#attrs)*
                            async #capture {
                                #(#inner)*
                                #block
                            }
                        }
                    });
                } else {
                    body.body = Box::new({
                        let old_body = &body.body;
                        parse_quote! {
                            {
                                #(#inner)*
                                #old_body
                            }
                        }
                    });
                }
                parse_quote_spanned! { Span::mixed_site() =>
                    {
                        #(#outer)*
                        #(#rename)*
                        #body
                    }
                }
            })
    }

    fn validate_pat_ident(&mut self, pat: syn::Pat) -> Option<syn::Ident> {
        match pat {
            syn::Pat::Ident(syn::PatIdent { ident, .. }) => Some(ident),
            _ => {
                self.errors
                    .push_spanned(pat, "Pattern for captured variable must be an identifier");
                None
            }
        }
    }

    fn get_captures(&mut self, inputs: &mut Vec<syn::Pat>, mode: Mode) -> Option<Vec<Capture>> {
        let mut captures = Vec::new();
        let mut names = HashSet::new();
        for pat in &*inputs {
            if let syn::Pat::Ident(syn::PatIdent { ident, .. }) = pat {
                if !pat_attrs(pat).iter().any(|attrs| {
                    attrs.iter().any(|a| {
                        a.path.is_ident("strong")
                            || a.path.is_ident("weak")
                            || (mode != Mode::Clone && a.path.is_ident("watch"))
                    })
                }) {
                    names.insert(ident.clone());
                }
            } else {
                extract_idents(pat, &mut names);
            }
        }
        let mut index = 0;
        let mut has_watch = false;
        while index < inputs.len() {
            let mut strong = None;
            let mut weak = None;
            let mut watch = None;
            if let Some(attrs) = pat_attrs_mut(&mut inputs[index]) {
                let index = attrs.iter().position(|a| {
                    a.path.is_ident("strong")
                        || a.path.is_ident("weak")
                        || (mode != Mode::Clone && a.path.is_ident("watch"))
                });
                if let Some(index) = index {
                    let attr = attrs.remove(index);
                    if attr.path.is_ident("strong") {
                        strong = Some(attr);
                    } else if attr.path.is_ident("weak") {
                        weak = Some(attr);
                    } else if attr.path.is_ident("watch") {
                        if !has_watch {
                            watch = Some(attr);
                            has_watch = true;
                        } else {
                            self.errors.push_spanned(
                                attr,
                                "Only one #[watch] attribute is allowed on closure",
                            );
                        }
                    }
                    for attr in attrs {
                        self.errors.push_spanned(
                            attr,
                            "Extra attributes not allowed after #[strong] or #[weak] or #[watch]",
                        );
                    }
                }
            }
            if let Some(strong) = strong {
                let span = strong.span();
                let from = parse_strong.parse2(strong.tokens).unwrap_or_else(|e| {
                    self.errors.push_syn(e);
                    None
                });
                let pat = inputs.remove(index);
                if from.is_some() && matches!(pat, syn::Pat::Wild(_)) {
                    captures.push(Capture::Strong { ident: None, from });
                } else if let Some(ident) = self.validate_pat_ident(pat) {
                    if names.contains(&ident) {
                        self.errors.push_spanned(
                            &ident,
                            format!(
                                "Identifier `{}` is used more than once in this parameter list",
                                ident
                            ),
                        );
                    } else {
                        names.insert(ident.clone());
                        captures.push(Capture::Strong {
                            ident: Some(ident),
                            from,
                        });
                    }
                } else {
                    self.errors.push(
                        span,
                        "capture must be named or provide a source expression using #[strong(...)]",
                    );
                }
            } else if let Some(weak) = weak {
                let span = weak.span();
                let (from, or) = parse_weak.parse2(weak.tokens).unwrap_or_else(|e| {
                    self.errors.push_syn(e);
                    (None, None)
                });
                let pat = inputs.remove(index);
                if from.is_some() && matches!(pat, syn::Pat::Wild(_)) {
                    let or = or.map(Rc::new);
                    captures.push(Capture::Weak {
                        ident: None,
                        from,
                        or,
                    });
                } else if let Some(ident) = self.validate_pat_ident(pat) {
                    if names.contains(&ident) {
                        self.errors.push_spanned(
                            &ident,
                            format!(
                                "Identifier `{}` is used more than once in this parameter list",
                                ident
                            ),
                        );
                    } else {
                        names.insert(ident.clone());
                        let or = or.map(Rc::new);
                        captures.push(Capture::Weak {
                            ident: Some(ident),
                            from,
                            or,
                        });
                    }
                } else {
                    self.errors.push(
                        span,
                        "capture must be named or provide a source expression using #[weak(...)]",
                    );
                }
            } else if let Some(watch) = watch {
                let span = watch.span();
                let from = parse_strong.parse2(watch.tokens).unwrap_or_else(|e| {
                    self.errors.push_syn(e);
                    None
                });
                let pat = inputs.remove(index);
                if from.is_some() && matches!(pat, syn::Pat::Wild(_)) {
                    captures.push(Capture::Watch { ident: None, from });
                } else if let Some(ident) = self.validate_pat_ident(pat) {
                    if names.contains(&ident) {
                        self.errors.push_spanned(
                            &ident,
                            format!(
                                "Identifier `{}` is used more than once in this parameter list",
                                ident
                            ),
                        );
                    } else {
                        names.insert(ident.clone());
                        captures.push(Capture::Watch {
                            ident: Some(ident),
                            from,
                        });
                    }
                } else {
                    self.errors.push(
                        span,
                        "capture must be named or provide a source expression using #[watch(...)]",
                    );
                }
            } else {
                index += 1;
            }
        }
        if captures.is_empty() {
            None
        } else {
            Some(captures)
        }
    }

    fn get_default_fail_action(
        &mut self,
        attrs: &mut Vec<syn::Attribute>,
    ) -> Option<UpgradeFailAction> {
        let index = attrs.iter().position(|syn::Attribute { path: p, .. }| {
            p.is_ident("default_panic")
                || p.is_ident("default_allow_none")
                || p.is_ident("default_return")
        });
        if let Some(index) = index {
            let attr = attrs.remove(index);
            if attr.path.is_ident("default_panic") {
                if let Err(e) = syn::parse2::<syn::parse::Nothing>(attr.tokens) {
                    self.errors.push_syn(e);
                }
                return Some(UpgradeFailAction::Panic);
            }
            if attr.path.is_ident("default_allow_none") {
                if let Err(e) = syn::parse2::<syn::parse::Nothing>(attr.tokens) {
                    self.errors.push_syn(e);
                }
                return Some(UpgradeFailAction::AllowNone);
            }
            if attr.path.is_ident("default_return") {
                let ret = (|input: syn::parse::ParseStream<'_>| {
                    if input.is_empty() {
                        return Ok(None);
                    }
                    let content;
                    syn::parenthesized!(content in input);
                    let expr = content.parse::<syn::Expr>()?;
                    content.parse::<syn::parse::Nothing>()?;
                    input.parse::<syn::parse::Nothing>()?;
                    Ok(Some(expr))
                })
                .parse2(attr.tokens);
                match ret {
                    Ok(expr) => return Some(UpgradeFailAction::Return(expr)),
                    Err(e) => self.errors.push_syn(e),
                }
            }
        }
        None
    }
}

impl<'v> VisitMut for Visitor<'v> {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        let new_expr = if let syn::Expr::Closure(closure) = expr {
            syn::visit_mut::visit_expr_mut(self, closure.body.as_mut());
            self.create_gclosure(closure)
                .or_else(|| self.create_closure(closure))
        } else {
            syn::visit_mut::visit_expr_mut(self, expr);
            None
        };
        if let Some(new_expr) = new_expr {
            *expr = new_expr;
        }
    }
}

pub fn closures(item: &mut syn::Item, crate_ident: &syn::Ident, errors: &Errors) {
    let mut visitor = Visitor {
        crate_ident,
        errors,
    };
    visitor.visit_item_mut(item);
}
