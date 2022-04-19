use crate::util::Errors;
use darling::util::{Flag, SpannedValue};
use proc_macro2::Span;

#[inline]
pub fn check_spanned<T>(value: &Option<SpannedValue<T>>) -> Option<Span> {
    value.as_ref().map(|v| v.span())
}

#[inline]
pub fn check_flag(flag: &SpannedValue<Flag>) -> Option<Span> {
    flag.is_some().then(|| flag.span())
}

#[inline]
pub fn check_bool(flag: &SpannedValue<Option<bool>>) -> Option<Span> {
    flag.is_some().then(|| flag.span())
}

pub fn disallow<'t>(
    name: &str,
    flags: impl IntoIterator<Item = &'t (&'static str, Option<Span>)>,
    errors: &Errors,
) {
    for (attr_name, span) in flags.into_iter() {
        if let Some(span) = *span {
            errors.push(span, format!("`{}` not allowed on {}", attr_name, name));
        }
    }
}

pub fn only_one<'t>(
    flags: impl IntoIterator<Item = &'t (&'static str, Option<Span>)> + Clone,
    errors: &Errors,
) {
    let present_spans = flags
        .clone()
        .into_iter()
        .filter_map(|f| f.1)
        .collect::<Vec<_>>();
    if present_spans.len() > 1 {
        let names = flags.into_iter().fold(String::new(), |a, (n, _)| {
            let n = format!("`{}`", n);
            if a.is_empty() {
                n
            } else {
                format!("{}, {}", a, n)
            }
        });
        for span in present_spans {
            errors.push(span, format!("Only one of {} is allowed", names));
        }
    }
}
