use darling::{util::Flag, FromAttributes, FromMeta};
use gobject_core::{
    util::{self, Errors},
    ClassDefinition, TypeContext, TypeMode,
};
use proc_macro2::{Span, TokenStream};
use quote::{quote, quote_spanned};
use std::collections::HashMap;
use syn::{parse_quote, parse_quote_spanned, spanned::Spanned};

#[derive(FromMeta)]
enum TemplateSource {
    File(syn::LitStr),
    String(syn::LitStr),
    Resource(syn::LitStr),
}

#[derive(Default, FromAttributes)]
#[darling(default, attributes(template_child))]
struct TemplateChildAttrs {
    id: Option<syn::LitStr>,
    internal: Flag,
}

struct TemplateChild {
    id: String,
    span: Span,
    internal: bool,
    field: syn::Expr,
    ty: syn::Type,
}

impl TemplateChild {
    fn many_from_fields<'f>(
        fields: impl Iterator<Item = &'f mut syn::Field>,
        errors: &Errors,
    ) -> Vec<Self> {
        let mut children = Vec::new();
        for (index, field) in fields.enumerate() {
            if let Some(attrs) = util::extract_attrs(&mut field.attrs, "template_child") {
                let attrs = util::parse_attributes::<TemplateChildAttrs>(&attrs, errors);
                Self::from_field(field, index, attrs, &mut children, errors);
            }
        }
        children
    }
    fn from_field(
        field: &mut syn::Field,
        index: usize,
        attrs: TemplateChildAttrs,
        children: &mut Vec<Self>,
        errors: &Errors,
    ) {
        let id = attrs
            .id
            .map(|id| (id.value(), id.span()))
            .or_else(|| field.ident.as_ref().map(|i| (i.to_string(), i.span())));
        let (id, span) = match id {
            Some(id) => id,
            None => {
                errors.push_spanned(
                    field,
                    "Unnamed field must have #[template_child(id = \"...\")]",
                );
                return;
            }
        };
        if children.iter().any(|c| c.id == id) {
            errors.push(span, format!("Duplicate template child with id `{}`", id));
        }
        let ty = field.ty.clone();
        let field = field
            .ident
            .as_ref()
            .map(|i| parse_quote_spanned! { i.span() => #i })
            .unwrap_or_else(|| parse_quote_spanned! { field.span() => #index });
        children.push(Self {
            id,
            span,
            internal: attrs.internal.is_some(),
            field,
            ty,
        });
    }
    fn bind_tokens(&self, class_ident: &syn::Ident, go: &syn::Path) -> TokenStream {
        let id = &self.id;
        let internal = &self.internal;
        let field = &self.field;
        quote_spanned! { self.field.span() =>
            #go::gtk4::subclass::prelude::WidgetClassSubclassExt::bind_template_child_with_offset(
                #class_ident,
                #id,
                #internal,
                #go::gtk4::offset_of!(Self => #field),
            );
        }
    }
    fn check_tokens(&self, this_ident: &syn::Ident, go: &syn::Path) -> TokenStream {
        let id = &self.id;
        let ty = &self.ty;
        let field = &self.field;
        let ty_ident = syn::Ident::new("ty", Span::mixed_site());
        let child_ty_ident = syn::Ident::new("ty", Span::mixed_site());
        quote_spanned! { ty.span() => {
            let #ty_ident = <<#ty as ::std::ops::Deref>::Target as #go::glib::StaticType>::static_type();
            let #child_ty_ident = #go::glib::object::ObjectExt::type_(::std::ops::Deref::deref(&#this_ident.#field));
            if !#child_ty_ident.is_a(#ty_ident) {
                ::std::panic!(
                    "Template child with id `{}` has incompatible type. XML has {:?}, struct has {:?}",
                    #id,
                    #child_ty_ident,
                    #ty_ident
                );
            }
        } }
    }
}

#[derive(Default, FromAttributes)]
#[darling(default, attributes(template_callbacks))]
struct TemplateCallbacksAttrs {
    functions: Flag,
}

#[derive(Default, FromAttributes)]
#[darling(default, attributes(template_callback))]
struct TemplateCallbackAttrs {
    name: Option<syn::LitStr>,
    function: Option<bool>,
}

struct TemplateCallback {
    name: String,
    sig: syn::Signature,
    rest_index: Option<usize>,
    mode: TypeMode,
    function: bool,
}

impl TemplateCallback {
    fn many_from_items(
        items: &mut Vec<syn::ImplItem>,
        mode: TypeMode,
        functions: bool,
        callbacks: &mut Vec<Self>,
        errors: &Errors,
    ) {
        for item in items {
            if let syn::ImplItem::Method(method) = item {
                if let Some(attrs) = util::extract_attrs(&mut method.attrs, "template_callback") {
                    let attrs = util::parse_attributes::<TemplateCallbackAttrs>(&attrs, errors);
                    Self::from_method(method, attrs, mode, functions, callbacks, errors);
                }
            }
        }
    }
    #[inline]
    fn from_method(
        method: &mut syn::ImplItemMethod,
        attrs: TemplateCallbackAttrs,
        mode: TypeMode,
        functions: bool,
        callbacks: &mut Vec<Self>,
        errors: &Errors,
    ) {
        let (name, span) = attrs
            .name
            .map(|n| (n.value(), n.span()))
            .unwrap_or_else(|| (method.sig.ident.to_string(), method.sig.ident.span()));
        if callbacks.iter().any(|c| c.name == name) {
            errors.push(
                span,
                format!("Duplicate template callback with name `{}`", name),
            );
        }
        if let (Some(_), syn::ReturnType::Type(_, ty)) =
            (method.sig.asyncness.as_ref(), &method.sig.output)
        {
            errors.push_spanned(ty, "Return value not allowed on async template callbacks");
        }
        match method.sig.receiver() {
            Some(syn::FnArg::Receiver(recv)) => {
                if let (Some(_), Some(mut_)) = (recv.reference.as_ref(), recv.mutability.as_ref()) {
                    errors.push_spanned(mut_, "Template callback receiver cannot be `&mut self`");
                }
            }
            Some(syn::FnArg::Typed(recv)) => {
                if let syn::Type::Reference(recv) = recv.ty.as_ref() {
                    if let Some(mut_) = recv.mutability.as_ref() {
                        errors
                            .push_spanned(mut_, "Template callback receiver cannot be `&mut self`");
                    }
                }
            }
            _ => {}
        }
        let mut rest_index = None;
        for (index, arg) in method.sig.inputs.iter_mut().enumerate() {
            if let syn::FnArg::Typed(arg) = arg {
                if let Some(attr) = util::extract_attr(&mut arg.attrs, "rest") {
                    util::require_empty(&attr, errors);
                    rest_index = Some(index);
                    break;
                }
            }
        }
        callbacks.push(Self {
            name,
            sig: method.sig.clone(),
            rest_index,
            mode,
            function: attrs.function.unwrap_or(functions),
        });
    }
    fn to_tokens(&self, wrapper_ty: &syn::Type, sub_ty: &syn::Type, go: &syn::Path) -> TokenStream {
        let name = &self.name;
        let start = if self.function { 1 } else { 0 };
        let values_ident = syn::Ident::new("values", Span::mixed_site());

        let assert_value_count = (!self.sig.inputs.is_empty()).then(|| {
            let required_value_count = self.sig.inputs.len() + start;
            quote_spanned! { self.sig.span() =>
                if #values_ident.len() < #required_value_count {
                    ::std::panic!(
                        "Template callback called with wrong number of arguments: Expected {}, got {}",
                        #required_value_count,
                        #values_ident.len(),
                    );
                }
            }
        });
        let arg_names = self
            .sig
            .inputs
            .iter()
            .enumerate()
            .map(|(index, _)| quote::format_ident!("value{}", index, span = Span::mixed_site()))
            .collect::<Vec<_>>();
        let arg_unwraps = self.sig.inputs.iter().enumerate().map(|(index, arg)| {
            let ident = &arg_names[index];
            let value_index = index + start;
            if Some(&*arg) == self.sig.receiver() {
                let ref_ = util::arg_reference(arg);
                let unwrap_recv = (self.mode == TypeMode::Subclass).then(|| {
                    quote_spanned! { arg.span() =>
                        let #ident = #go::glib::subclass::prelude::ObjectSubclassIsExt::imp(#ident);
                    }
                });
                quote_spanned! { arg.span() =>
                    let #ident = #values_ident[#value_index]
                        .get::<#ref_ #wrapper_ty>()
                        .unwrap_or_else(|e| ::std::panic!(
                                "Wrong type for `self` in template callback `{}`: {:?}",
                                #name,
                                e
                        ));
                    #unwrap_recv
                }
            } else if Some(index) == self.rest_index {
                quote_spanned! { arg.span() =>
                    let #ident = &#values_ident[#value_index..#values_ident.len()];
                }
            } else {
                let ty = match arg {
                    syn::FnArg::Typed(t) => &t.ty,
                    syn::FnArg::Receiver(_) => wrapper_ty,
                };
                quote_spanned! { arg.span() =>
                    let #ident = #values_ident[#value_index]
                        .get::<#ty>()
                        .unwrap_or_else(|e| ::std::panic!(
                                "Wrong type for argument {} in template callback `{}`: {:?}",
                                #index,
                                #name,
                                e
                        ));
                }
            }
        });

        let ident = &self.sig.ident;
        let dest = match self.mode {
            TypeMode::Subclass => sub_ty,
            TypeMode::Wrapper => wrapper_ty,
        };
        let call = quote_spanned! { self.sig.span() =>
            #dest::#ident(#(#arg_names),*)
        };
        let body = match (&self.sig.asyncness, &self.sig.output) {
            (None, syn::ReturnType::Default) => quote_spanned! { self.sig.span() =>
                #(#arg_unwraps)*
                #call;
                ::std::option::Option::None
            },
            (None, syn::ReturnType::Type(_, _)) => quote_spanned! { self.sig.span() =>
                #(#arg_unwraps)*
                ::std::option::Option::Some(
                    #go::glib::value::ToValue::to_value(&#call)
                )
            },
            (Some(_), _) => quote_spanned! { self.sig.span() =>
                let #values_ident = #values_ident.to_vec();
                #go::glib::MainContext::default().spawn_local(async move {
                    #(#arg_unwraps)*
                    #call.await;
                });
                ::std::option::Option::None
            },
        };
        quote_spanned! { self.sig.span() =>
            (#name, |#values_ident| {
                #assert_value_count
                #body
            })
        }
    }
}

impl TemplateSource {
    fn to_tokens(&self, go: &syn::Path) -> TokenStream {
        let class_ident = syn::Ident::new("class", Span::mixed_site());
        match self {
            Self::File(file) => quote_spanned! { file.span() =>
                #go::gtk4::subclass::widget::WidgetClassSubclassExt::set_template_static(
                    #class_ident,
                    include_bytes!(#file),
                );
            },
            Self::String(string) => quote_spanned! { string.span() =>
                #go::gtk4::subclass::widget::WidgetClassSubclassExt::set_template_static(
                    #class_ident,
                    #string.as_bytes(),
                );
            },
            Self::Resource(resource) => quote_spanned! { resource.span() =>
                #go::gtk4::subclass::widget::WidgetClassSubclassExt::set_template_from_resource(
                    #class_ident,
                    &#resource,
                );
            },
        }
    }
    fn check_children(&self, children: &[TemplateChild], errors: &Errors) {
        let xml = match self {
            Self::String(string) => string.value(),
            _ => return,
        };

        let mut reader = quick_xml::Reader::from_str(&xml);
        let mut buf = Vec::new();
        let mut ids_left = children
            .iter()
            .map(|c| (c.id.as_str(), c.span))
            .collect::<HashMap<_, _>>();

        loop {
            use quick_xml::events::Event;

            let event = reader.read_event(&mut buf);
            let elem = match &event {
                Ok(Event::Start(e)) => Some(e),
                Ok(Event::Empty(e)) => Some(e),
                Ok(Event::Eof) => break,
                Err(e) => {
                    errors.push(
                        self.span(),
                        format!(
                            "Failed reading template XML at position {}: {:?}",
                            reader.buffer_position(),
                            e
                        ),
                    );
                    break;
                }
                _ => None,
            };
            if let Some(e) = elem {
                let name = e.name();
                if name == b"object" || name == b"template" {
                    let id = e
                        .attributes()
                        .find_map(|a| a.ok().and_then(|a| (a.key == b"id").then(|| a)));
                    let id = id.as_ref().and_then(|a| std::str::from_utf8(&a.value).ok());
                    if let Some(id) = id {
                        ids_left.remove(id);
                    }
                }
            }

            buf.clear();
        }

        for (name, span) in ids_left {
            errors.push(
                span,
                format!(
                    "Template child with id `{}` not found in template XML",
                    name
                ),
            );
        }
    }
}

impl Spanned for TemplateSource {
    fn span(&self) -> Span {
        match self {
            Self::File(file) => file.span(),
            Self::String(string) => string.span(),
            Self::Resource(resource) => resource.span(),
        }
    }
}

pub(crate) fn extend_template(def: &mut ClassDefinition, errors: &Errors) {
    let (name, source) = match (|| {
        let name = def.inner.name.clone()?;
        let struct_ = def.inner.properties_item_mut()?;
        let attr = util::extract_attr(&mut struct_.attrs, "template")?;
        let source = util::parse_paren_list_optional::<TemplateSource>(attr.tokens, errors)?;
        Some((name, source))
    })() {
        Some(a) => a,
        None => return,
    };

    let children = def
        .inner
        .properties_item_mut()
        .map(|struct_| TemplateChild::many_from_fields(struct_.fields.iter_mut(), errors))
        .unwrap_or_default();
    source.check_children(&children, errors);

    let mut callbacks = Vec::new();
    for impl_ in def.inner.methods_items_mut() {
        if let Some(mode) = TypeMode::for_item_type(&*impl_.self_ty) {
            let attrs = util::extract_attrs(&mut impl_.attrs, "template_callbacks")
                .map(|attrs| util::parse_attributes::<TemplateCallbacksAttrs>(&attrs, errors))
                .unwrap_or_default();
            TemplateCallback::many_from_items(
                &mut impl_.items,
                mode,
                attrs.functions.is_some(),
                &mut callbacks,
                errors,
            );
        }
    }

    let has_callbacks = callbacks.iter().any(|c| c.mode == TypeMode::Subclass);
    let has_instance_callbacks = callbacks.iter().any(|c| c.mode == TypeMode::Wrapper);
    let class_ident = syn::Ident::new("class", Span::mixed_site());
    let this_ident = syn::Ident::new("obj", Span::mixed_site());
    let widget_ident = syn::Ident::new("_widget", Span::mixed_site());
    let go = def.inner.crate_path.clone();
    let go = &go;
    let gtk4 = quote::quote! { #go::gtk4 };
    let bind_template = source.to_tokens(go);
    let bind_template_children = children.iter().map(|c| c.bind_tokens(&class_ident, go));
    let check_template_children = children.iter().map(|c| c.check_tokens(&widget_ident, go));
    let bind_template_callbacks = has_callbacks.then(|| quote_spanned! { Span::mixed_site() =>
        #gtk4::subclass::widget::CompositeTemplateCallbacksClass::bind_template_callbacks(#class_ident);
    });
    let bind_instance_callbacks = has_instance_callbacks.then(|| quote_spanned! { Span::mixed_site() =>
        #gtk4::subclass::widget::CompositeTemplateInstanceCallbacksClass::bind_template_instance_callbacks(#class_ident);
    });
    def.inner.add_custom_stmt(
        "class_init",
        parse_quote_spanned! { Span::mixed_site() => {
            #bind_template
            unsafe {
                #(#bind_template_children)*
            };
            #bind_template_callbacks
            #bind_instance_callbacks
        }; },
    );
    def.inner.add_custom_stmt(
        "instance_init",
        parse_quote_spanned! { Span::mixed_site() => {
            let #widget_ident = unsafe { #this_ident.as_ref() };
            #gtk4::prelude::WidgetExt::init_template(#widget_ident);
            let #widget_ident = #gtk4::subclass::prelude::ObjectSubclassIsExt::imp(#widget_ident);
            #(#check_template_children)*
        }; },
    );
    if !callbacks.is_empty() {
        let wrapper_ty =
            def.inner
                .type_(TypeMode::Subclass, TypeMode::Wrapper, TypeContext::External);
        let sub_ty = def.inner.type_(
            TypeMode::Subclass,
            TypeMode::Subclass,
            TypeContext::External,
        );
        let wrapper_ty = parse_quote! { #wrapper_ty };
        let sub_ty = parse_quote! { #sub_ty };
        if has_callbacks {
            let head = def.inner.trait_head(
                &parse_quote! { #name },
                quote! { #gtk4::subclass::widget::CompositeTemplateCallbacks },
            );
            let callbacks = callbacks.iter().filter_map(|c| {
                (c.mode == TypeMode::Subclass).then(|| c.to_tokens(&wrapper_ty, &sub_ty, go))
            });
            let item = syn::Item::Verbatim(quote_spanned! { source.span() =>
                #head {
                    const CALLBACKS: &'static [#gtk4::subclass::widget::TemplateCallback] = &[
                        #(#callbacks),*
                    ];
                }
            });
            def.inner.ensure_items().push(item);
        }
        if has_instance_callbacks {
            let head = def.inner.trait_head(
                &parse_quote! { super::#name },
                quote! { #gtk4::subclass::widget::CompositeTemplateCallbacks },
            );
            let callbacks = callbacks.iter().filter_map(|c| {
                (c.mode == TypeMode::Wrapper).then(|| c.to_tokens(&wrapper_ty, &sub_ty, go))
            });
            let item = syn::Item::Verbatim(quote_spanned! { source.span() =>
                #head {
                    const CALLBACKS: &'static [#gtk4::subclass::widget::TemplateCallback] = &[
                        #(#callbacks),*
                    ];
                }
            });
            def.inner.ensure_items().push(item);
        }
    }
}
