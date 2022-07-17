use crate::util::{self, Errors};
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use std::collections::HashSet;
use std::rc::Rc;
use syn::{
    parse::{ParseStream, Parser},
    parse_quote, parse_quote_spanned,
    spanned::Spanned,
    visit_mut::VisitMut,
};

enum Capture {
    Strong {
        span: Span,
        ident: Option<syn::Ident>,
        from: Option<syn::Expr>,
    },
    Weak {
        span: Span,
        ident: Option<syn::Ident>,
        from: Option<syn::Expr>,
        or: Option<Rc<UpgradeFailAction>>,
    },
    Watch {
        span: Span,
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
    fn ident(&self) -> Option<&syn::Ident> {
        match self {
            Self::Strong { ident, .. } => ident.as_ref(),
            Self::Weak { ident, .. } => ident.as_ref(),
            Self::Watch { ident, .. } => ident.as_ref(),
        }
    }
    fn set_default_fail(&mut self, action: &Rc<UpgradeFailAction>) {
        if let Self::Weak { or, .. } = self {
            if or.is_none() {
                *or = Some(action.clone());
            }
        }
    }
    fn outer_tokens(&self, index: usize, go: &syn::Path) -> Option<TokenStream> {
        Some(match self {
            Self::Strong { ident, from, .. } => {
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
            Self::Watch { ident, from, .. } => {
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
    fn inner_tokens(&self, index: usize, mode: Mode, go: &syn::Path) -> Option<TokenStream> {
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
    fn after_tokens(&self, go: &syn::Path) -> Option<TokenStream> {
        Some(match self {
            Self::Watch { ident, from, .. } if ident.is_some() || from.is_some() => {
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

impl Spanned for Capture {
    fn span(&self) -> Span {
        match self {
            Self::Strong { span, .. } => *span,
            Self::Weak { span, .. } => *span,
            Self::Watch { span, .. } => *span,
        }
    }
}

fn extract_idents<'p>(pat: &'p syn::Pat, idents: &mut HashSet<&'p syn::Ident>) {
    use syn::Pat::*;
    match pat {
        Box(p) => extract_idents(&*p.pat, idents),
        Ident(p) => {
            idents.insert(&p.ident);
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
    syn::custom_keyword!(weak);
    syn::custom_keyword!(strong);
    syn::custom_keyword!(watch);
    syn::custom_keyword!(or);
    syn::custom_keyword!(or_panic);
    syn::custom_keyword!(or_return);
    syn::custom_keyword!(allow_none);
    syn::custom_keyword!(default_panic);
    syn::custom_keyword!(default_return);
    syn::custom_keyword!(default_allow_none);
}

#[derive(Default)]
struct CloneAttrs {
    captures: Vec<Capture>,
    or: Option<(Span, UpgradeFailAction)>,
}

#[derive(Default)]
struct ClosureAttrs {
    local: bool,
    captures: Vec<Capture>,
    or: Option<(Span, UpgradeFailAction)>,
}

fn parse_clone(input: syn::parse::ParseStream<'_>, errors: &Errors) -> syn::Result<CloneAttrs> {
    let mut or = None;
    let mut captures = Vec::new();
    if !input.is_empty() {
        let content;
        syn::parenthesized!(content in input);
        while !content.is_empty() {
            let lookahead = content.lookahead1();
            if lookahead.peek(keywords::weak)
                || lookahead.peek(keywords::strong)
                || lookahead.peek(keywords::watch)
            {
                captures.push(parse_capture(&content, Mode::Clone)?);
            } else if lookahead.peek(keywords::default_panic)
                || lookahead.peek(keywords::default_allow_none)
                || lookahead.peek(keywords::default_return)
            {
                if or.is_some() {
                    let span = content.parse::<syn::Ident>()?.span();
                    errors.push(span, "Duplicate default action specified");
                }
                or = Some(parse_default_action(&content)?);
            } else {
                return Err(lookahead.error());
            };
            if !content.is_empty() {
                content.parse::<syn::Token![,]>()?;
            }
        }
    }
    Ok(CloneAttrs { captures, or })
}

fn parse_default_action(
    input: syn::parse::ParseStream<'_>,
) -> syn::Result<(Span, UpgradeFailAction)> {
    let lookahead = input.lookahead1();
    if lookahead.peek(keywords::default_panic) {
        let span = input.parse::<keywords::default_panic>()?.span();
        Ok((span, UpgradeFailAction::Panic))
    } else if lookahead.peek(keywords::default_allow_none) {
        let span = input.parse::<keywords::default_allow_none>()?.span();
        Ok((span, UpgradeFailAction::AllowNone))
    } else if lookahead.peek(keywords::default_return) {
        let span = input.parse::<keywords::default_return>()?.span();
        let expr = if input.peek(syn::token::Paren) {
            let content;
            syn::parenthesized!(content in input);
            let expr = content.parse::<syn::Expr>()?;
            content.parse::<syn::parse::Nothing>()?;
            Some(expr)
        } else {
            None
        };
        Ok((span, UpgradeFailAction::Return(expr)))
    } else {
        Err(lookahead.error())
    }
}

fn parse_capture(input: syn::parse::ParseStream<'_>, mode: Mode) -> syn::Result<Capture> {
    let lookahead = input.lookahead1();
    if lookahead.peek(keywords::strong) {
        let span = input.parse::<keywords::strong>()?.span();
        let from = if input.peek(syn::token::Paren) {
            parse_strong(input)?
        } else {
            None
        };
        let ident = if input.peek(syn::Token![_]) {
            input.parse::<syn::Token![_]>()?;
            None
        } else {
            Some(input.parse()?)
        };
        Ok(Capture::Strong { span, ident, from })
    } else if lookahead.peek(keywords::weak) {
        let span = input.parse::<keywords::weak>()?.span();
        let (from, or) = if input.peek(syn::token::Paren) {
            parse_weak(input)?
        } else {
            (None, None)
        };
        let ident = if input.peek(syn::Token![_]) {
            input.parse::<syn::Token![_]>()?;
            None
        } else {
            Some(input.parse()?)
        };
        Ok(Capture::Weak {
            span,
            ident,
            from,
            or: or.map(Rc::new),
        })
    } else if mode != Mode::Clone && lookahead.peek(keywords::watch) {
        let span = input.parse::<keywords::watch>()?.span();
        let from = if input.peek(syn::token::Paren) {
            parse_strong(input)?
        } else {
            None
        };
        let ident = if input.peek(syn::Token![_]) {
            input.parse::<syn::Token![_]>()?;
            None
        } else {
            Some(input.parse()?)
        };
        Ok(Capture::Watch { span, ident, from })
    } else {
        Err(lookahead.error())
    }
}

fn parse_closure(input: syn::parse::ParseStream<'_>, errors: &Errors) -> syn::Result<ClosureAttrs> {
    let mut or = None;
    let mut captures = Vec::new();
    let mut local = false;
    if !input.is_empty() {
        let content;
        syn::parenthesized!(content in input);
        while !content.is_empty() {
            let lookahead = content.lookahead1();
            if lookahead.peek(keywords::local) {
                content.parse::<keywords::local>()?;
                local = true;
            } else if lookahead.peek(keywords::weak)
                || lookahead.peek(keywords::strong)
                || lookahead.peek(keywords::watch)
            {
                captures.push(parse_capture(&content, Mode::Closure)?);
            } else if lookahead.peek(keywords::default_panic)
                || lookahead.peek(keywords::default_allow_none)
                || lookahead.peek(keywords::default_return)
            {
                if or.is_some() {
                    let span = content.parse::<syn::Ident>()?.span();
                    errors.push(span, "Duplicate default action specified");
                }
                or = Some(parse_default_action(&content)?);
            } else {
                return Err(lookahead.error());
            };
            if !content.is_empty() {
                content.parse::<syn::Token![,]>()?;
            }
        }
    }
    Ok(ClosureAttrs {
        local,
        captures,
        or,
    })
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

fn has_captures<'p>(mut inputs: impl Iterator<Item = &'p syn::Pat>) -> bool {
    inputs.any(|pat| {
        util::pat_attrs(pat).iter().any(|attrs| {
            attrs
                .iter()
                .any(|a| a.path.is_ident("strong") || a.path.is_ident("weak"))
        })
    })
}

struct Visitor<'v> {
    crate_path: &'v syn::Path,
    errors: &'v Errors,
}

impl<'v> Visitor<'v> {
    fn create_gclosure(&mut self, closure: &syn::ExprClosure) -> Option<syn::Expr> {
        let has_closure = closure.attrs.iter().any(|a| a.path.is_ident("closure"));
        let has_watch = closure.inputs.iter().any(|pat| {
            util::pat_attrs(pat)
                .iter()
                .any(|attrs| attrs.iter().any(|a| a.path.is_ident("watch")))
        });
        if !has_closure && !has_watch {
            return None;
        }

        let mut attrs = closure.attrs.clone();
        let mut captures = Vec::new();
        let mut local = !has_closure;
        let mut action = None;
        if let Some(attrs) = util::extract_attrs(&mut attrs, "closure") {
            for attr in attrs {
                let attrs = syn::parse::Parser::parse2(
                    |stream: ParseStream<'_>| parse_closure(stream, self.errors),
                    attr.tokens,
                )
                .map_err(|e| self.errors.push_syn(e))
                .unwrap_or_default();
                captures.extend(attrs.captures);
                if let Some((span, or)) = attrs.or {
                    if action.is_some() {
                        self.errors.push(span, "Duplicate default action specified");
                    }
                    action = Some(or);
                }
                local = attrs.local || local;
            }
        }

        let mode = match closure.body.as_ref() {
            syn::Expr::Async(_) => Mode::ClosureAsync,
            _ => Mode::Closure,
        };
        let mut inputs = closure.inputs.iter().cloned().collect::<Vec<_>>();
        if let Some(caps) = self.get_captures(&mut inputs, mode) {
            captures.extend(caps);
        }
        self.extract_default_fail_action(&mut attrs, &mut action);
        if let Some(action) = action {
            let action = Rc::new(action);
            for capture in &mut captures {
                capture.set_default_fail(&action);
            }
        }
        if !captures.is_empty() && closure.capture.is_none() {
            self.errors.push_spanned(
                closure,
                "Closure must be `move` to use #[watch] or #[strong] or #[weak]",
            );
        }
        self.validate_captures(&captures, &inputs);

        let mut rest_index = None;
        for (index, pat) in inputs.iter_mut().enumerate() {
            if let Some(attrs) = util::pat_attrs_mut(pat) {
                if let Some(attr) = util::extract_attr(attrs, "rest") {
                    util::require_empty(&attr, self.errors);
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

        let go = self.crate_path;
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
        let required_arg_count = inputs
            .iter()
            .enumerate()
            .rev()
            .find_map(|(i, p)| {
                (Some(i) != rest_index && !matches!(p, syn::Pat::Wild(_))).then(|| i + 1)
            })
            .unwrap_or(0);
        let assert_arg_count = (required_arg_count > 0).then(|| {
            quote! {
                if #values_ident.len() < #required_arg_count {
                    ::std::panic!(
                        "Closure called with wrong number of arguments: Expected {}, got {}",
                        #required_arg_count,
                        #values_ident.len(),
                    );
                }
            }
        });
        let arg_unwraps = inputs.iter().enumerate().map(|(index, pat)| match pat {
            syn::Pat::Wild(_) => None,
            _ => {
                let attrs = util::pat_attrs(pat).into_iter().flat_map(|a| a.iter());
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
            #assert_arg_count
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
        let has_clone = closure.attrs.iter().any(|a| a.path.is_ident("clone"));
        if !has_clone && !has_captures(closure.inputs.iter()) {
            return None;
        }
        let mut captures = Vec::new();
        let mut attrs = closure.attrs.clone();
        let mut action = None;
        if let Some(attrs) = util::extract_attrs(&mut attrs, "clone") {
            for attr in attrs {
                let attrs = syn::parse::Parser::parse2(
                    |stream: ParseStream<'_>| parse_clone(stream, self.errors),
                    attr.tokens,
                )
                .map_err(|e| self.errors.push_syn(e))
                .unwrap_or_default();
                captures.extend(attrs.captures);
                if let Some((span, or)) = attrs.or {
                    if action.is_some() {
                        self.errors.push(span, "Duplicate default action specified");
                    }
                    action = Some(or);
                }
            }
        }
        let mut inputs = closure.inputs.iter().cloned().collect::<Vec<_>>();
        if let Some(caps) = self.get_captures(&mut inputs, Mode::Clone) {
            captures.extend(caps);
        }
        self.validate_captures(&captures, &inputs);
        if closure.capture.is_none() {
            self.errors.push_spanned(
                closure,
                "Closure must be `move` to use #[strong] or #[weak]",
            );
        }
        self.extract_default_fail_action(&mut attrs, &mut action);
        if let Some(action) = action {
            let action = Rc::new(action);
            for capture in &mut captures {
                capture.set_default_fail(&action);
            }
        }
        let go = self.crate_path;
        let outer = captures
            .iter()
            .enumerate()
            .map(|(i, c)| c.outer_tokens(i, go));
        let rename = captures.iter().enumerate().map(|(i, c)| c.rename_tokens(i));
        let inner = captures
            .iter()
            .enumerate()
            .map(|(i, c)| c.inner_tokens(i, Mode::Clone, go));
        let output;
        let body = if let syn::Expr::Async(syn::ExprAsync {
            attrs,
            capture,
            block,
            ..
        }) = &*closure.body
        {
            output = syn::ReturnType::Default;
            let block = match &closure.output {
                syn::ReturnType::Type(_, ty) => {
                    let ret = syn::Ident::new("____ret", Span::mixed_site());
                    quote! {
                        let #ret: #ty = #block;
                        #ret
                    }
                }
                _ => quote! { #block },
            };
            parse_quote! {
                #(#attrs)*
                async #capture {
                    #(#inner)*
                    #block
                }
            }
        } else {
            output = closure.output.clone();
            let old_body = &closure.body;
            parse_quote! {
                {
                    #(#inner)*
                    #old_body
                }
            }
        };
        let body = syn::ExprClosure {
            attrs,
            movability: closure.movability,
            asyncness: closure.asyncness,
            capture: closure.capture,
            or1_token: closure.or1_token,
            inputs: FromIterator::from_iter(inputs.into_iter()),
            or2_token: closure.or2_token,
            output,
            body: Box::new(body),
        };
        Some(parse_quote_spanned! { Span::mixed_site() =>
            {
                #(#outer)*
                #(#rename)*
                #body
            }
        })
    }

    fn create_async(&mut self, async_: &syn::ExprAsync) -> Option<syn::Expr> {
        let has_clone = async_.attrs.iter().any(|a| a.path.is_ident("clone"));
        if !has_clone {
            return None;
        }
        let mut captures = Vec::new();
        let mut action = None;
        let mut attrs = async_.attrs.clone();
        if let Some(attrs) = util::extract_attrs(&mut attrs, "clone") {
            for attr in attrs {
                let attrs = syn::parse::Parser::parse2(
                    |stream: ParseStream<'_>| parse_clone(stream, self.errors),
                    attr.tokens,
                )
                .map_err(|e| self.errors.push_syn(e))
                .unwrap_or_default();
                captures.extend(attrs.captures);
                if let Some((span, or)) = attrs.or {
                    if action.is_some() {
                        self.errors.push(span, "Duplicate default action specified");
                    }
                    action = Some(or);
                }
            }
        }
        self.validate_captures(&captures, &[]);
        if async_.capture.is_none() {
            self.errors
                .push_spanned(async_, "Async block must be `move` to use #[clone]");
        }
        self.extract_default_fail_action(&mut attrs, &mut action);
        if let Some(action) = action {
            let action = Rc::new(action);
            for capture in &mut captures {
                capture.set_default_fail(&action);
            }
        }
        let go = self.crate_path;
        let outer = captures
            .iter()
            .enumerate()
            .map(|(i, c)| c.outer_tokens(i, go));
        let rename = captures.iter().enumerate().map(|(i, c)| c.rename_tokens(i));
        let inner = captures
            .iter()
            .enumerate()
            .map(|(i, c)| c.inner_tokens(i, Mode::Clone, go));
        let block = &async_.block;
        let block = parse_quote! {
            {
                #(#inner)*
                #block
            }
        };
        let body = syn::ExprAsync {
            attrs,
            async_token: async_.async_token,
            capture: async_.capture,
            block,
        };
        Some(parse_quote_spanned! { Span::mixed_site() =>
            {
                #(#outer)*
                #(#rename)*
                #body
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

    fn validate_captures(&mut self, captures: &[Capture], inputs: &[syn::Pat]) {
        let mut has_watch = false;
        let mut names = HashSet::new();
        for pat in inputs {
            extract_idents(pat, &mut names);
        }
        for capture in captures {
            if let Capture::Watch { span, .. } = capture {
                if has_watch {
                    self.errors
                        .push(*span, "Only one #[watch] attribute is allowed on closure");
                } else {
                    has_watch = true;
                }
            }
            if let Some(ident) = capture.ident() {
                if names.contains(ident) {
                    self.errors.push_spanned(
                        ident,
                        format!(
                            "Identifier `{}` is used more than once in this parameter list",
                            ident
                        ),
                    );
                } else {
                    names.insert(ident);
                }
            }
        }
    }

    fn get_captures(&mut self, inputs: &mut Vec<syn::Pat>, mode: Mode) -> Option<Vec<Capture>> {
        let mut captures = Vec::new();
        let mut index = 0;
        while index < inputs.len() {
            let mut strong = None;
            let mut weak = None;
            let mut watch = None;
            if let Some(attrs) = util::pat_attrs_mut(&mut inputs[index]) {
                if let Some(attr) = util::extract_attr(attrs, "strong") {
                    strong = Some(attr);
                } else if let Some(attr) = util::extract_attr(attrs, "weak") {
                    weak = Some(attr);
                } else if mode != Mode::Clone {
                    if let Some(attr) = util::extract_attr(attrs, "watch") {
                        watch = Some(attr);
                    }
                }
                if strong.is_some() || weak.is_some() || watch.is_some() {
                    for attr in attrs {
                        self.errors.push_spanned(
                            attr,
                            "Extra attributes not allowed on #[strong] or #[weak] or #[watch] capture",
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
                let ident = if matches!(pat, syn::Pat::Wild(_)) {
                    None
                } else {
                    self.validate_pat_ident(pat)
                };
                if ident.is_some() || from.is_some() {
                    captures.push(Capture::Strong { span, ident, from });
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
                let ident = if matches!(pat, syn::Pat::Wild(_)) {
                    None
                } else {
                    self.validate_pat_ident(pat)
                };
                if ident.is_some() || from.is_some() {
                    captures.push(Capture::Weak {
                        span,
                        ident,
                        from,
                        or: or.map(Rc::new),
                    });
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
                let ident = if matches!(pat, syn::Pat::Wild(_)) {
                    None
                } else {
                    self.validate_pat_ident(pat)
                };
                if ident.is_some() || from.is_some() {
                    captures.push(Capture::Watch { span, ident, from });
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

    fn extract_default_fail_action(
        &mut self,
        attrs: &mut Vec<syn::Attribute>,
        action_out: &mut Option<UpgradeFailAction>,
    ) {
        loop {
            let action = if let Some(attr) = util::extract_attr(attrs, "default_panic") {
                let span = attr.span();
                if let Err(e) = syn::parse2::<syn::parse::Nothing>(attr.tokens) {
                    self.errors.push_syn(e);
                }
                Some((span, UpgradeFailAction::Panic))
            } else if let Some(attr) = util::extract_attr(attrs, "default_allow_none") {
                let span = attr.span();
                if let Err(e) = syn::parse2::<syn::parse::Nothing>(attr.tokens) {
                    self.errors.push_syn(e);
                }
                Some((span, UpgradeFailAction::AllowNone))
            } else if let Some(attr) = util::extract_attr(attrs, "default_return") {
                let span = attr.span();
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
                    Ok(expr) => Some((span, UpgradeFailAction::Return(expr))),
                    Err(e) => {
                        self.errors.push_syn(e);
                        None
                    }
                }
            } else {
                None
            };
            if let Some((span, action)) = action {
                if action_out.is_some() {
                    self.errors.push(span, "Duplicate default action specified");
                }
                *action_out = Some(action);
            } else {
                break;
            }
        }
    }

    fn visit_one(&mut self, expr: &mut syn::Expr) {
        if let syn::Expr::Closure(closure) = expr {
            let new_expr = self
                .create_gclosure(closure)
                .or_else(|| self.create_closure(closure));
            if let Some(new_expr) = new_expr {
                *expr = new_expr;
            }
        };
    }
}

impl<'v> VisitMut for Visitor<'v> {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        let new_expr = if let syn::Expr::Closure(closure) = expr {
            syn::visit_mut::visit_expr_mut(self, closure.body.as_mut());
            self.create_gclosure(closure)
                .or_else(|| self.create_closure(closure))
        } else if let syn::Expr::Async(async_) = expr {
            self.create_async(async_)
        } else {
            syn::visit_mut::visit_expr_mut(self, expr);
            None
        };
        if let Some(new_expr) = new_expr {
            *expr = new_expr;
        }
    }
}

pub fn closures(item: &mut syn::Item, crate_path: &syn::Path, errors: &Errors) {
    let mut visitor = Visitor { crate_path, errors };
    visitor.visit_item_mut(item);
}

pub fn closure_expr(expr: &mut syn::Expr, crate_path: &syn::Path, errors: &Errors) {
    let mut visitor = Visitor { crate_path, errors };
    visitor.visit_one(expr);
}
