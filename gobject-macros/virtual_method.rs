use std::collections::HashSet;

pub struct VirtualMethod {
    method: syn::ImplItemMethod,
}

impl VirtualMethod {
    pub fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        errors: &mut Vec<darling::Error>,
    ) -> Vec<Self> {
        let mut virtual_method_names = HashSet::new();
        let mut virtual_methods = Vec::<VirtualMethod>::new();

        let mut index = 0;
        loop {
            if index >= items.len() {
                break;
            }
            let mut method_attr = None;
            if let syn::ImplItem::Method(method) = &mut items[index] {
                let method_index = method
                    .attrs
                    .iter()
                    .position(|attr| attr.path.is_ident("virtual"));
                if let Some(method_index) = method_index {
                    method_attr.replace(method.attrs.remove(method_index));
                }
                if let Some(next) = method.attrs.first() {
                    errors.push(
                        syn::Error::new_spanned(next, "Unknown attribute on virtual method").into(),
                    );
                }
            }
            if let Some(attr) = method_attr {
                let sub = items.remove(index);
                let mut method = match sub {
                    syn::ImplItem::Method(method) => method,
                    _ => unreachable!(),
                };
                let virtual_method =
                    Self::from_method(method, attr, &mut virtual_method_names, errors);
                virtual_methods.push(virtual_method);
            } else {
                index += 1;
            }
        }

        virtual_methods
    }
    #[inline]
    fn from_method<'methods>(
        method: syn::ImplItemMethod,
        attr: syn::Attribute,
        virtual_method_names: &mut HashSet<String>,
        errors: &mut Vec<darling::Error>,
    ) -> Self {
        if !attr.tokens.is_empty() {
            errors.push(
                syn::Error::new_spanned(&attr.tokens, "Unknown tokens on accumulator").into(),
            );
        }
        {
            let ident = &method.sig.ident;
            if virtual_method_names.contains(&ident.to_string()) {
                errors.push(
                    syn::Error::new_spanned(
                        ident,
                        format!("Duplicate definition for method `{}`", ident),
                    )
                    .into(),
                );
            }
        }
        if method.sig.receiver().is_none() {
            if let Some(first) = method.sig.inputs.first() {
                errors.push(
                    syn::Error::new_spanned(
                        first,
                        "First argument to method handler must be `&self`",
                    )
                    .into(),
                );
            }
        }
        Self { method }
    }
}
