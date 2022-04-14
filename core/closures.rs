use crate::util::Errors;
use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote, quote_spanned, ToTokens};
use syn::{parse::Parser, parse_quote, parse_quote_spanned, spanned::Spanned, visit_mut::VisitMut};

struct WatchCapture {
    ident: Option<syn::Ident>,
    from: Option<syn::Expr>,
}

struct StrongCapture {
    ident: syn::Ident,
    from: Option<syn::Expr>,
}

#[derive(Clone)]
enum UpgradeFailAction {
    AllowNone,
    Panic,
    Default(syn::Expr),
    Return(Option<syn::Expr>),
}

struct WeakCapture {
    ident: syn::Ident,
    from: Option<syn::Expr>,
    or: Option<UpgradeFailAction>,
}

#[derive(Default)]
struct Captures {
    strong: Vec<StrongCapture>,
    weak: Vec<WeakCapture>,
}

impl Captures {
    fn set_default_fail(&mut self, action: &UpgradeFailAction) {
        for weak in &mut self.weak {
            if weak.or.is_none() {
                weak.or = Some(action.clone());
            }
        }
    }
    fn outer_tokens(&self, go: &syn::Ident) -> Vec<TokenStream> {
        let strongs = self.strong.iter().map(|s| {
            let StrongCapture { ident, from } = s;
            let from = from
                .as_ref()
                .map(|f| f.to_token_stream())
                .unwrap_or_else(|| ident.to_token_stream());
            quote! { let #ident = ::std::clone::Clone::clone(&#from); }
        });
        let weaks = self.weak.iter().map(|w| {
            let WeakCapture { ident, from, .. } = w;
            let from = from
                .as_ref()
                .map(|f| f.to_token_stream())
                .unwrap_or_else(|| ident.to_token_stream());
            quote! { let #ident = #go::glib::clone::Downgrade::downgrade(&#from); }
        });
        strongs.chain(weaks).collect()
    }
    fn inner_tokens(&self, go: &syn::Ident, is_closure: bool) -> Vec<TokenStream> {
        self.weak
            .iter()
            .map(|WeakCapture { ident, or, .. }| {
                let upgrade = quote! { #go::glib::clone::Upgrade::upgrade(&#ident) };
                let upgrade = match or {
                    None | Some(UpgradeFailAction::AllowNone) => upgrade,
                    Some(or) => {
                        let action = match or {
                            UpgradeFailAction::Panic => {
                                let name = ident.to_string();
                                quote! { ::std::panic!("Failed to upgrade `{}`", #name) }
                            }
                            UpgradeFailAction::Default(expr) => expr.to_token_stream(),
                            UpgradeFailAction::Return(expr) => if is_closure {
                                quote! {
                                    return #go::glib::closure::ToClosureReturnValue::to_closure_return_value(
                                        &#expr
                                    )
                                }
                            } else {
                                quote! { return #expr }
                            },
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
            })
            .collect()
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
    crate_ident: syn::Ident,
    errors: &'v Errors,
}

impl<'v> Visitor<'v> {
    fn create_gclosure(&mut self, closure: &syn::ExprClosure) -> Option<syn::Expr> {
        let closure_index = closure
            .attrs
            .iter()
            .position(|a| a.path.is_ident("closure"));
        let watch_index = closure.inputs.iter().position(|pat| {
            pat_attrs(pat)
                .map(|attrs| attrs.iter().any(|a| a.path.is_ident("watch")))
                .unwrap_or(false)
        });
        if closure_index.is_none() && watch_index.is_none() {
            return None;
        }
        let has_captures = has_captures(closure.inputs.iter());
        if (has_captures || watch_index.is_some()) && closure.capture.is_none() {
            self.errors.push_spanned(
                closure,
                "Closure must be `move` to use #[watch] or #[strong] or #[weak]",
            );
        }

        let mut body = closure.clone();
        body.capture = None;
        let local = if let Some(closure_index) = closure_index {
            let mut attrs = closure.attrs.clone();
            let attr = attrs.remove(closure_index);
            body.attrs = attrs;
            parse_closure.parse2(attr.tokens).unwrap_or_else(|e| {
                self.errors.push_syn(e);
                false
            })
        } else {
            true
        };
        let mut watch = None;
        let mut captures = if watch_index.is_some() || has_captures {
            let mut inputs = closure.inputs.iter().cloned().collect::<Vec<_>>();
            if let Some(watch_index) = watch_index {
                let mut pat = inputs.remove(watch_index);
                let attrs = pat_attrs_mut(&mut pat).unwrap();
                let index = attrs.iter().position(|a| a.path.is_ident("watch")).unwrap();
                let strong = attrs.remove(index);
                let span = strong.span();
                for attr in attrs {
                    self.errors.push_spanned(
                        attr,
                        "Extra attributes not allowed after #[watch]",
                    );
                }
                let from = parse_strong.parse2(strong.tokens).unwrap_or_else(|e| {
                    self.errors.push_syn(e);
                    None
                });
                if matches!(pat, syn::Pat::Wild(_)) {
                    watch = Some(WatchCapture { ident: None, from });
                } else if let Some(ident) = self.validate_pat_ident(pat) {
                    watch = Some(WatchCapture {
                        ident: Some(ident),
                        from,
                    });
                } else {
                    self.errors.push(
                        span,
                        "#[watch] capture must be named or provide a source expression using #[watch(...)]",
                    );
                }
            }
            let captures = if has_captures {
                self.get_captures(&mut inputs)
            } else {
                None
            };
            body.inputs = FromIterator::from_iter(inputs.into_iter());
            captures
        } else {
            None
        }
        .unwrap_or_default();

        if let Some(action) = self.get_default_fail_action(&mut body) {
            captures.set_default_fail(&action);
        }

        if watch_index.is_some() {
            for pat in &body.inputs {
                if let Some(attrs) = pat_attrs(pat) {
                    for attr in attrs {
                        if attr.path.is_ident("watch") {
                            self.errors.push_spanned(
                                attr,
                                "Only one watch capture is allowed per closure",
                            );
                        }
                    }
                }
            }
        }

        let go = &self.crate_ident;
        let watch_ident = syn::Ident::new("____watch", Span::mixed_site());
        let closure_ident = syn::Ident::new("____closure", Span::mixed_site());
        let values_ident = syn::Ident::new("____values", Span::mixed_site());
        let constructor = if local {
            format_ident!("new_local")
        } else {
            format_ident!("new")
        };
        let outer = captures.outer_tokens(go);
        let inner = captures.inner_tokens(go, true);
        let watch_outer = watch.as_ref().map(|WatchCapture { ident, from }| {
            let target = ident
                .as_ref()
                .map(|i| i.to_token_stream())
                .unwrap_or_else(|| watch_ident.to_token_stream());
            let from = from
                .as_ref()
                .map(|f| f.to_token_stream())
                .unwrap_or_else(|| target.clone());
            quote! {
                let #target = #go::glib::object::Watchable::watched_object(&#from);
                let #watch_ident = unsafe { #target.borrow() };
                let #watch_ident = ::std::convert::AsRef::as_ref(&#watch_ident);
            }
        });
        let watch_inner = watch.as_ref().and_then(|WatchCapture { ident, .. }| {
            let target = ident.as_ref()?;
            Some(quote! {
                let #target = unsafe { #target.borrow() };
                let #target = ::std::convert::AsRef::as_ref(&#target);
            })
        });
        let watch_after = watch.as_ref().map(|_| {
            quote! {
                #go::glib::object::ObjectExt::watch_closure(#watch_ident, &#closure_ident);
            }
        });
        let args_len = body.inputs.len();
        let arg_names = body
            .inputs
            .iter()
            .enumerate()
            .map(|(i, _)| format_ident!("arg{}", i));
        let arg_values = arg_names.clone().enumerate().map(|(index, arg)| {
            quote! {
                let #arg = #go::glib::Value::get(&#values_ident[#index])
                    .unwrap_or_else(|e| {
                        ::std::panic!("Wrong type for closure argument {}: {:?}", #index, e)
                    });
            }
        });
        Some(parse_quote_spanned! { Span::mixed_site() =>
            {
                #(#outer)*
                #watch_outer
                let #closure_ident = #go::glib::closure::RustClosure::#constructor(move |#values_ident| {
                    if #values_ident.len() != #args_len {
                        ::std::panic!(
                            "Closure called with wrong number of arguments: Expected {}, got {}",
                            #args_len,
                            #values_ident.len(),
                        );
                    }
                    #(#inner)*
                    #watch_inner
                    #(#arg_values)*
                    #go::glib::closure::ToClosureReturnValue::to_closure_return_value(
                        &(#body)(#(#arg_names),*)
                    )
                });
                #watch_after
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
        self.get_captures(&mut inputs).map(|mut captures| {
            let mut body = closure.clone();
            if let Some(action) = self.get_default_fail_action(&mut body) {
                captures.set_default_fail(&action);
            }
            let go = &self.crate_ident;
            let outer = captures.outer_tokens(go);
            let inner = captures.inner_tokens(go, false);
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

    fn get_captures(&mut self, inputs: &mut Vec<syn::Pat>) -> Option<Captures> {
        let mut captures = Captures::default();
        let mut index = 0;
        while index < inputs.len() {
            let mut strong = None;
            let mut weak = None;
            if let Some(attrs) = pat_attrs_mut(&mut inputs[index]) {
                let index = attrs
                    .iter()
                    .position(|a| a.path.is_ident("strong") || a.path.is_ident("weak"));
                if let Some(index) = index {
                    let attr = attrs.remove(index);
                    if attr.path.is_ident("strong") {
                        strong = Some(attr);
                    } else {
                        weak = Some(attr);
                    }
                    for attr in attrs {
                        self.errors.push_spanned(
                            attr,
                            "Extra attributes not allowed after #[strong] or #[weak]",
                        );
                    }
                }
            }
            if let Some(strong) = strong {
                let from = parse_strong.parse2(strong.tokens).unwrap_or_else(|e| {
                    self.errors.push_syn(e);
                    None
                });
                let pat = inputs.remove(index);
                if let Some(ident) = self.validate_pat_ident(pat) {
                    captures.strong.push(StrongCapture { ident, from });
                }
            } else if let Some(weak) = weak {
                let (from, or) = parse_weak.parse2(weak.tokens).unwrap_or_else(|e| {
                    self.errors.push_syn(e);
                    (None, None)
                });
                let pat = inputs.remove(index);
                if let Some(ident) = self.validate_pat_ident(pat) {
                    captures.weak.push(WeakCapture { ident, from, or });
                }
            } else {
                index += 1;
            }
        }
        if captures.strong.is_empty() && captures.weak.is_empty() {
            None
        } else {
            Some(captures)
        }
    }

    fn get_default_fail_action(
        &mut self,
        closure: &mut syn::ExprClosure,
    ) -> Option<UpgradeFailAction> {
        let index = closure
            .attrs
            .iter()
            .position(|syn::Attribute { path: p, .. }| {
                p.is_ident("default_panic")
                    || p.is_ident("default_allow_none")
                    || p.is_ident("default_return")
            });
        if let Some(index) = index {
            let attr = closure.attrs.remove(index);
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

pub fn closures(item: &mut syn::Item, crate_ident: syn::Ident, errors: &Errors) {
    let mut visitor = Visitor {
        crate_ident,
        errors,
    };
    visitor.visit_item_mut(item);
}
