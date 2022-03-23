use crate::util;
use darling::util::{Flag, SpannedValue};
use proc_macro2::Span;

#[inline]
pub(crate) fn check_flag(flag: &SpannedValue<Flag>) -> Option<Span> {
    flag.is_some().then(|| flag.span())
}

#[inline]
pub(crate) fn check_bool(flag: &SpannedValue<Option<bool>>) -> Option<Span> {
    flag.is_some().then(|| flag.span())
}

pub(crate) fn disallow<'t>(
    name: &str,
    flags: impl IntoIterator<Item = &'t (&'static str, Option<Span>)>,
    errors: &mut Vec<darling::Error>,
) {
    for (attr_name, span) in flags.into_iter() {
        if let Some(span) = *span {
            util::push_error(
                errors,
                span,
                format!("`{}` not allowed on {}", attr_name, name),
            );
        }
    }
}

pub(crate) fn only_one<'t>(
    flags: impl IntoIterator<Item = &'t (&'static str, Option<Span>)> + Clone,
    errors: &mut Vec<darling::Error>,
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
            util::push_error(errors, span, format!("Only one of {} is allowed", names));
        }
    }
}
