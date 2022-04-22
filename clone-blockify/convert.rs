use proc_macro2::{LineColumn, Span, TokenStream};
use quote::{ToTokens, TokenStreamExt};
use std::{fmt::Write, ops::Range};
use syn::{ext::IdentExt, parse::Parser, spanned::Spanned, visit::Visit, Token};
use tokio::io::AsyncWriteExt;

struct StrongCapture {
    ident: syn::Ident,
    from: Option<syn::Expr>,
}

impl StrongCapture {
    fn to_string(&self, name: &str, source: &Source<'_>) -> String {
        let from = self
            .from
            .as_ref()
            .and_then(|f| source.string_for_spanned(f))
            .map(|f| format!("({})", f))
            .unwrap_or_default();
        format!("#[{}{}] {}", name, from, self.ident)
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
    fn to_string(&self, extra: Option<&str>, source: &Source<'_>) -> String {
        let mut from = [
            self.from
                .as_ref()
                .and_then(|f| source.string_for_spanned(f)),
            extra,
        ]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
        if !from.is_empty() {
            from = format!("({})", from);
        }
        format!("#[weak{}] {}", from, self.ident)
    }
}

struct Closure {
    watch: Option<StrongCapture>,
    strong: Vec<StrongCapture>,
    weak: Vec<WeakCapture>,
    action: Option<DefaultAction>,
    body: syn::Expr,
}

impl Closure {
    fn to_string(
        &self,
        source: &Source<'_>,
        local: Option<bool>,
        preserve_default: bool,
    ) -> String {
        let mut output = String::new();
        let attrs = match &self.body {
            syn::Expr::Async(async_) => &async_.attrs,
            syn::Expr::Closure(closure) => &closure.attrs,
            _ => return String::new(),
        };
        for attr in attrs {
            if let Some(s) = source.string_for_spanned(attr) {
                std::write!(&mut output, "{} ", s).unwrap();
            }
        }
        match local {
            Some(true) if self.watch.is_none() => output.push_str("#[closure(local)] "),
            Some(false) => output.push_str("#[closure] "),
            _ => {}
        };
        let default_weaks = self.weak.iter().filter(|w| !w.allow_none).count();
        let mut has_default = false;
        if preserve_default || default_weaks > 1 {
            match &self.action {
                Some(DefaultAction::Panic) => {
                    output.push_str("#[default_panic] ");
                    has_default = true;
                }
                Some(DefaultAction::Return(expr)) => {
                    if let Some(expr) = source.string_for_spanned(expr) {
                        std::write!(&mut output, "#[default_return({})] ", expr).unwrap();
                        has_default = true;
                    }
                }
                None => {
                    if !preserve_default {
                        output.push_str("#[default_return] ");
                        has_default = true;
                    }
                }
            }
        }
        let mut arg_count = match &self.body {
            syn::Expr::Async(_) => {
                output.push_str("move |");
                0
            }
            syn::Expr::Closure(closure) => {
                if let Some(movability) = closure
                    .movability
                    .as_ref()
                    .and_then(|m| source.string_for_spanned(m))
                {
                    std::write!(&mut output, "{} ", movability).unwrap();
                }
                if let Some(asyncness) = closure
                    .asyncness
                    .as_ref()
                    .and_then(|a| source.string_for_spanned(a))
                {
                    std::write!(&mut output, "{} ", asyncness).unwrap();
                }
                if let Some(capture) = closure
                    .capture
                    .as_ref()
                    .and_then(|c| source.string_for_spanned(c))
                {
                    std::write!(&mut output, "{} ", capture).unwrap();
                }
                output.push('|');
                let arg_count = closure.inputs.len();
                for (index, pat) in closure.inputs.iter().enumerate() {
                    if let Some(s) = source.string_for_spanned(pat) {
                        output.push_str(s);
                        if index != arg_count - 1 {
                            output.push_str(", ");
                        }
                    }
                }
                arg_count
            }
            _ => 0,
        };

        if let Some(watch) = &self.watch {
            if arg_count > 0 {
                output.push_str(", ");
            }
            output.push_str(&watch.to_string("watch", source));
            arg_count += 1;
        }
        for strong in &self.strong {
            if arg_count > 0 {
                output.push_str(", ");
            }
            output.push_str(&strong.to_string("strong", source));
            arg_count += 1;
        }
        for weak in &self.weak {
            let arg = match (weak.allow_none, has_default) {
                (true, true) => weak.to_string(Some("allow_none"), source),
                (false, false) => match &self.action {
                    Some(DefaultAction::Panic) => weak.to_string(Some("or_panic"), source),
                    Some(DefaultAction::Return(expr)) => {
                        let mut extra = String::from("or_return");
                        if let Some(s) = source.string_for_spanned(expr) {
                            extra.push(' ');
                            extra.push_str(s);
                        }
                        weak.to_string(Some(&extra), source)
                    }
                    None => weak.to_string(Some("or_return"), source),
                },
                _ => weak.to_string(None, source),
            };
            if arg_count > 0 {
                output.push_str(", ");
            }
            output.push_str(&arg);
            arg_count += 1;
        }
        output.push_str("| ");
        match &self.body {
            syn::Expr::Async(async_) => {
                if let Some(s) = source.string_for_spanned(&async_.block) {
                    output.push_str(s);
                }
            }
            syn::Expr::Closure(closure) => {
                if let syn::ReturnType::Type(_, ty) = &closure.output {
                    if let Some(s) = source.string_for_spanned(&*ty) {
                        std::write!(&mut output, "-> {} ", s).unwrap();
                    }
                }
                if let Some(s) = source.string_for_spanned(&*closure.body) {
                    output.push_str(s);
                }
            }
            _ => {}
        }
        output
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
                    return Err(syn::Error::new_spanned(ident, "Duplicate `watch` capture"));
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

struct Source<'s> {
    full: &'s str,
    lines: Vec<&'s str>,
}

impl<'s> Source<'s> {
    fn position(&self, pos: LineColumn) -> Option<usize> {
        self.lines.get(pos.line - 1).and_then(|l| {
            let index = l
                .char_indices()
                .nth(pos.column)
                .map(|i| i.0)
                .unwrap_or_else(|| l.len());
            Some(l.get(index..index)?.as_ptr() as usize - self.full.as_ptr() as usize)
        })
    }
    fn range_for(&self, span: Span) -> Option<Range<usize>> {
        Some(self.position(span.start())?..self.position(span.end())?)
    }
    fn string_for(&self, span: Span) -> Option<&'s str> {
        self.full.get(self.range_for(span)?)
    }
    fn string_for_spanned(&self, spanned: &impl Spanned) -> Option<&'s str> {
        self.string_for(spanned.span())
    }
}

struct Visitor<'s> {
    source: Source<'s>,
    preserve_default: bool,
    replacements: Vec<(Range<usize>, String)>,
    errors: Vec<syn::Error>,
}

impl<'s> Visitor<'s> {
    fn convert_clone(&mut self, tokens: TokenStream) -> Option<String> {
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
        Some(closure.to_string(&self.source, None, self.preserve_default))
    }
    fn convert_closure(&mut self, tokens: TokenStream, local: bool) -> Option<String> {
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
        Some(closure.to_string(&self.source, Some(local), self.preserve_default))
    }
}

impl<'ast, 's> Visit<'ast> for Visitor<'s> {
    fn visit_expr(&mut self, expr: &'ast syn::Expr) {
        let output = if let syn::Expr::Macro(mac) = expr {
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
            syn::visit::visit_expr(self, expr);
            None
        };
        if let Some(output) = output {
            if let Some(range) = self.source.range_for(expr.span()) {
                self.replacements.push((range, output));
            }
        }
    }
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
            std::format_args!("{}:{}: {}\n{}\n{:>pos$}", sline, column, source, line, "^",)
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

pub(crate) async fn convert(
    source: &str,
    preserve_default: bool,
) -> anyhow::Result<Option<String>> {
    let mut errors = Vec::new();
    let file = match syn::parse_str::<syn::File>(source) {
        Ok(file) => file,
        Err(e) => {
            errors.push(e);
            syn::File {
                shebang: None,
                attrs: Vec::new(),
                items: Vec::new(),
            }
        }
    };

    let mut visitor = Visitor {
        source: Source {
            full: source,
            lines: source.lines().collect(),
        },
        preserve_default,
        replacements: Vec::new(),
        errors,
    };
    for item in &file.items {
        let prev_replacements = visitor.replacements.len();
        visitor.visit_item(item);
        if prev_replacements < visitor.replacements.len() {
            if let Some(pos) = visitor.source.position(item.span().start()) {
                visitor.replacements.insert(
                    prev_replacements,
                    (pos..pos, "#[gobject::clone_block]\n".into()),
                );
            }
        }
    }

    if !visitor.errors.is_empty() {
        let mut parse_errors = ParseErrors::default();
        for err in visitor.errors {
            let start = err.span().start();
            parse_errors.0.push(ParseError::new(
                err,
                visitor.source.lines[start.line - 1].to_owned(),
            ));
        }
        return Err(parse_errors.into());
    }

    if !visitor.replacements.is_empty() {
        let mut source = visitor.source.full.to_owned();
        for (range, replacement) in visitor.replacements.into_iter().rev() {
            source.replace_range(range, &replacement);
        }
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
