use proc_macro2::{LineColumn, Span, TokenStream};
use quote::{quote, ToTokens, TokenStreamExt};
use syn::{
    ext::IdentExt, parse::Parser, parse_quote, spanned::Spanned, visit_mut::VisitMut, Token,
};
use tokio::io::AsyncWriteExt;

struct StrongCapture {
    ident: syn::Ident,
    from: Option<syn::Expr>,
}

impl StrongCapture {
    fn to_pat(&self, name: &str) -> syn::Pat {
        let from = self.from.as_ref().map(|f| quote! { (#f) });
        let name = quote::format_ident!("{}", name);
        let attr = parse_quote! { #[#name #from] };
        syn::Pat::Ident(syn::PatIdent {
            attrs: vec![attr],
            by_ref: None,
            mutability: None,
            ident: self.ident.clone(),
            subpat: None,
        })
    }
}

#[derive(Clone)]
enum DefaultAction {
    Panic,
    Return(syn::Expr),
}

struct WeakCapture {
    ident: syn::Ident,
    from: Option<syn::Expr>,
    allow_none: bool,
}

impl WeakCapture {
    fn to_pat(&self, extra: Option<TokenStream>) -> syn::Pat {
        let from = self
            .from
            .as_ref()
            .map(|f| quote! { (#f #extra) })
            .or_else(|| extra.as_ref().map(|e| quote! { (#e) }));
        let attr = parse_quote! { #[weak #from] };
        syn::Pat::Ident(syn::PatIdent {
            attrs: vec![attr],
            by_ref: None,
            mutability: None,
            ident: self.ident.clone(),
            subpat: None,
        })
    }
}

struct Closure {
    watch: Option<StrongCapture>,
    strong: Vec<StrongCapture>,
    weak: Vec<WeakCapture>,
    action: Option<DefaultAction>,
    body: syn::Expr,
}

impl ToTokens for Closure {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let mut closure = match &self.body {
            syn::Expr::Closure(closure) => closure.clone(),
            syn::Expr::Async(block) => parse_quote! { move || #block },
            _ => return,
        };
        let default_weaks = self.weak.iter().filter(|w| !w.allow_none).count();
        if default_weaks > 1 {
            match &self.action {
                Some(DefaultAction::Panic) => {
                    closure.attrs.push(parse_quote! { #[default_panic] });
                }
                Some(DefaultAction::Return(expr)) => {
                    closure
                        .attrs
                        .push(parse_quote! { #[default_return(#expr)] });
                }
                None => {
                    closure.attrs.push(parse_quote! { #[default_return] });
                }
            }
        }
        if let Some(watch) = &self.watch {
            closure.inputs.insert(0, watch.to_pat("watch"))
        }
        for strong in &self.strong {
            closure.inputs.insert(0, strong.to_pat("strong"))
        }
        for weak in &self.weak {
            let extra = if weak.allow_none && default_weaks > 1 {
                Some(quote! { allow_none })
            } else if !weak.allow_none && default_weaks == 1 {
                Some(match &self.action {
                    Some(DefaultAction::Panic) => quote! { or_panic },
                    Some(DefaultAction::Return(expr)) => quote! { or_return #expr },
                    None => quote! { or_return },
                })
            } else {
                None
            };
            closure.inputs.insert(0, weak.to_pat(extra));
        }
        closure.to_tokens(tokens);
    }
}

fn parse_dashed_keyword(input: syn::parse::ParseStream<'_>) -> syn::Result<(String, Span)> {
    let mut idents = TokenStream::new();
    idents.append(input.call(syn::Ident::parse_any)?);
    while input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        idents.append(input.call(syn::Ident::parse_any)?);
    }
    let keyword = idents
        .clone()
        .into_iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join("-");
    Ok((keyword, idents.span()))
}

fn parse_closure(input: syn::parse::ParseStream<'_>) -> syn::Result<Closure> {
    enum CaptureKind {
        Strong,
        Watch,
        Weak,
        WeakAllowNone,
    }

    let mut watch = None;
    let mut strong = Vec::new();
    let mut weak = Vec::new();

    while input.peek(Token![@]) {
        use CaptureKind::*;

        input.parse::<Token![@]>()?;
        let (keyword, kw_span) = parse_dashed_keyword(input)?;
        let kind = match keyword.as_str() {
            "strong" => Strong,
            "watch" => Watch,
            "weak" => Weak,
            "weak-allow-none" => WeakAllowNone,
            _ => return Err(syn::Error::new(kw_span, "Unknown capture type")),
        };
        let mut name = TokenStream::new();
        name.append(input.call(syn::Ident::parse_any)?);
        while input.peek(Token![.]) {
            name.append_all(input.parse::<Token![.]>()?.to_token_stream());
            name.append(input.call(syn::Ident::parse_any)?);
        }
        let (ident, from) = if input.peek(Token![as]) {
            input.parse::<Token![as]>()?;
            (input.call(syn::Ident::parse_any)?, Some(syn::parse2(name)?))
        } else {
            (syn::parse2(name)?, None)
        };
        match kind {
            Strong => strong.push(StrongCapture { ident, from }),
            Watch => {
                if watch.is_none() {
                    watch = Some(StrongCapture { ident, from });
                } else {
                    return Err(syn::Error::new_spanned(ident, "Duplicate `watch` c apture"));
                }
            }
            Weak => weak.push(WeakCapture {
                ident,
                from,
                allow_none: false,
            }),
            WeakAllowNone => weak.push(WeakCapture {
                ident,
                from,
                allow_none: true,
            }),
        };
        let lookahead = input.lookahead1();
        if lookahead.peek(Token![,]) {
            input.parse::<Token![,]>()?;
        } else if lookahead.peek(Token![=>]) {
            input.parse::<Token![=>]>()?;
            break;
        } else {
            return Err(lookahead.error());
        }
    }

    let action = if input.peek(Token![@]) {
        input.parse::<Token![@]>()?;
        let (keyword, kw_span) = parse_dashed_keyword(input)?;
        let action = match keyword.as_str() {
            "default-panic" => DefaultAction::Panic,
            "default-return" => DefaultAction::Return(input.parse()?),
            _ => return Err(syn::Error::new(kw_span, "Unknown keyword")),
        };
        input.parse::<Token![,]>()?;
        Some(action)
    } else {
        None
    };

    let body = input.parse()?;

    input.parse::<syn::parse::Nothing>()?;

    Ok(Closure {
        watch,
        strong,
        weak,
        action,
        body,
    })
}

#[derive(Default)]
struct Visitor {
    errors: Vec<syn::Error>,
    converted: bool,
}

impl Visitor {
    fn convert_clone(&mut self, tokens: TokenStream) -> Option<syn::Expr> {
        let closure = match parse_closure.parse2(tokens) {
            Ok(c) => c,
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };
        if !matches!(closure.body, syn::Expr::Closure(_) | syn::Expr::Async(_)) {
            self.errors.push(syn::Error::new_spanned(
                closure.body,
                "Unsupported expression type",
            ));
            return None;
        }
        Some(syn::parse2(closure.to_token_stream()).unwrap())
    }
    fn convert_closure(&mut self, tokens: TokenStream, local: bool) -> Option<syn::Expr> {
        let closure = match parse_closure.parse2(tokens) {
            Ok(c) => c,
            Err(e) => {
                self.errors.push(e);
                return None;
            }
        };
        if !matches!(closure.body, syn::Expr::Closure(_)) {
            self.errors.push(syn::Error::new_spanned(
                closure.body,
                "Unsupported expression type",
            ));
            return None;
        }
        let tokens = closure.to_token_stream();
        let attr = match local {
            true => quote! { #[closure(local)] },
            false => quote! { #[closure] },
        };
        Some(parse_quote! { #attr #tokens })
    }
}

impl VisitMut for Visitor {
    fn visit_expr_mut(&mut self, expr: &mut syn::Expr) {
        let new_expr = if let syn::Expr::Macro(mac) = expr {
            let path = mac
                .mac
                .path
                .to_token_stream()
                .into_iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("");
            if path == "clone" || path == "glib::clone" {
                self.convert_clone(mac.mac.tokens.clone())
            } else if path == "closure" || path == "glib::closure" {
                self.convert_closure(mac.mac.tokens.clone(), false)
            } else if path == "closure_local" || path == "glib::closure_local" {
                self.convert_closure(mac.mac.tokens.clone(), true)
            } else {
                None
            }
        } else {
            syn::visit_mut::visit_expr_mut(self, expr);
            None
        };
        if let Some(new_expr) = new_expr {
            self.converted = true;
            *expr = new_expr;
        }
    }
}

#[inline]
fn item_attrs_mut(item: &mut syn::Item) -> Option<&mut Vec<syn::Attribute>> {
    use syn::Item::*;
    use syn::*;
    Some(match item {
        Const(ItemConst { attrs, .. }) => attrs,
        Enum(ItemEnum { attrs, .. }) => attrs,
        ExternCrate(ItemExternCrate { attrs, .. }) => attrs,
        Fn(ItemFn { attrs, .. }) => attrs,
        ForeignMod(ItemForeignMod { attrs, .. }) => attrs,
        Impl(ItemImpl { attrs, .. }) => attrs,
        Macro(ItemMacro { attrs, .. }) => attrs,
        Macro2(ItemMacro2 { attrs, .. }) => attrs,
        Mod(ItemMod { attrs, .. }) => attrs,
        Static(ItemStatic { attrs, .. }) => attrs,
        Struct(ItemStruct { attrs, .. }) => attrs,
        Trait(ItemTrait { attrs, .. }) => attrs,
        TraitAlias(ItemTraitAlias { attrs, .. }) => attrs,
        Type(ItemType { attrs, .. }) => attrs,
        Union(ItemUnion { attrs, .. }) => attrs,
        Use(ItemUse { attrs, .. }) => attrs,
        _ => return None,
    })
}

#[derive(thiserror::Error, Debug, Default)]
pub struct ParseErrors(Vec<ParseError>);

impl std::fmt::Display for ParseErrors {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for ParseError { source, line } in &self.0 {
            let LineColumn {
                line: sline,
                column,
            } = source.span().start();
            let pos = column + 1;
            std::format_args!(
                "{}:{}: {}\n{}\n{:>pos$}",
                sline,
                column,
                source,
                line,
                "^",
            )
            .fmt(f)?;
        }
        Ok(())
    }
}

#[derive(derive_new::new, Debug)]
struct ParseError {
    source: syn::Error,
    line: String,
}

pub(crate) async fn convert(source: &str) -> anyhow::Result<Option<String>> {
    let mut visitor = Visitor::default();
    let mut converted = false;
    let mut file = match syn::parse_str::<syn::File>(source) {
        Ok(file) => file,
        Err(e) => {
            visitor.errors.push(e);
            syn::File {
                shebang: None,
                attrs: Vec::new(),
                items: Vec::new(),
            }
        }
    };

    for item in &mut file.items {
        visitor.visit_item_mut(item);
        if visitor.converted {
            converted = true;
            visitor.converted = false;
            if let Some(attrs) = item_attrs_mut(item) {
                attrs.push(parse_quote! { #[gobject::clone_block] });
            }
        }
    }

    if !visitor.errors.is_empty() {
        let lines = source.lines().collect::<Vec<_>>();
        let mut parse_errors = ParseErrors::default();
        for err in visitor.errors {
            let start = err.span().start();
            parse_errors
                .0
                .push(ParseError::new(err, lines[start.line - 1].to_owned()));
        }
        return Err(parse_errors.into());
    }

    if converted {
        let source = file.to_token_stream().to_string();
        let rustfmt = tokio::process::Command::new("rustfmt")
            .args(&["--edition", "2021"])
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .spawn();
        let source = if let Ok(mut rustfmt) = rustfmt {
            let mut stdin = rustfmt.stdin.take().unwrap();
            stdin.write_all(source.as_ref()).await?;
            std::mem::drop(stdin);
            let output = rustfmt.wait_with_output().await?;
            if !output.status.success() {
                let stderr = String::from_utf8_lossy(&output.stderr);
                return Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("rustfmt failed: {}", stderr),
                )
                .into());
            }
            String::from_utf8(output.stdout)?
        } else {
            source
        };
        Ok(Some(source))
    } else {
        Ok(None)
    }
}
