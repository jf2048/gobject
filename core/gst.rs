use crate::ClassOptions;
use crate::{ClassDefinition, TypeMode};
use gst;
use proc_macro2::{Span, TokenStream};
use quote::{quote, ToTokens};
use std::{ops::Deref, str::FromStr};

use super::{
    util::{self, Errors},
    ClassAttrs,
};
use darling::{util::PathList, FromMeta};
use syn::NestedMeta;

#[derive(Debug, Default, Clone, PartialEq)]
pub struct PadTemplates(Vec<PadTemplate>);

impl PadTemplates {
    pub fn new(templates: Vec<PadTemplate>) -> Self {
        PadTemplates(templates)
    }
}

impl Deref for PadTemplates {
    type Target = Vec<PadTemplate>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl FromMeta for PadTemplates {
    fn from_list(v: &[NestedMeta]) -> Result<Self, darling::Error> {
        let mut templates = Vec::with_capacity(v.len());
        for nmi in v {
            if let NestedMeta::Meta(syn::Meta::List(ref value)) = *nmi {
                if value.path.segments.len() != 1 {
                    return Err(
                        darling::Error::unknown_field("Expected 'pad_template'").with_span(nmi)
                    );
                }

                let mut template = PadTemplate::from_list(
                    &value
                        .nested
                        .iter()
                        .cloned()
                        .collect::<Vec<syn::NestedMeta>>()[..],
                )?;

                if template.name.is_some() {
                    return Err(darling::Error::unknown_field("name").with_span(nmi));
                }

                let name = value.path.segments.first().unwrap().ident.to_string();
                template.name = Some(name.replace("__", "_%"));
                if template.direction.is_none() {
                    template.direction = Some(if name.starts_with("src") {
                        Direction::Src
                    } else if name.starts_with("sink") {
                        Direction::Sink
                    } else {
                        return Err(darling::Error::custom("invalid 'value' for 'direction'")
                            .with_span(nmi));
                    });
                }

                if let Some(ref caps) = template.caps {
                    gst::Caps::from_str(caps).map_err(|err| {
                        darling::Error::custom(&format!("invalid 'caps': '{err:?}'")).with_span(nmi)
                    })?;
                }

                template
                    .presence
                    .as_ref()
                    .ok_or_else(|| darling::Error::missing_field("presence").with_span(nmi))?;

                templates.push(template);
            } else {
                return Err(darling::Error::unexpected_type("non-word").with_span(nmi));
            }
        }

        Ok(PadTemplates(templates))
    }
}

#[derive(Debug, PartialEq, Eq, FromMeta, Clone)]
#[darling(rename_all = "snake_case")]
pub enum Rank {
    None,
    Marginal,
    Secondary,
    Primary,
}

impl Rank {
    fn to_gst_toks(&self) -> TokenStream {
        match self {
            Rank::None => quote!(gst::Rank::None),
            Rank::Marginal => quote!(gst::Rank::Marginal),
            Rank::Secondary => quote!(gst::Rank::Secondary),
            Rank::Primary => quote!(gst::Rank::Primary),
        }
    }
}

#[derive(Debug, PartialEq, Eq, FromMeta, Clone)]
#[darling(rename_all = "snake_case")]
pub enum Presence {
    Always,
    Sometimes,
    Request,
}

#[derive(Debug, PartialEq, Eq, FromMeta, Clone, Default)]
#[darling(rename_all = "snake_case")]
pub enum Direction {
    #[default]
    Src,
    Sink,
}

#[derive(Debug, FromMeta, Eq, PartialEq, Clone, Default)]
pub struct PadTemplate {
    pub presence: Option<Presence>,
    #[darling(default)]
    pub direction: Option<Direction>,
    #[darling(default)]
    pub caps: Option<String>,
    #[darling(default)]
    pub name: Option<String>,
}

#[derive(Debug, Default, FromMeta)]
pub struct Attrs {
    pub factory_name: String,
    pub rank: Option<Rank>,
    pub class: Option<ClassAttrs>,
    pub long_name: String,
    pub description: String,
    pub classification: String,
    pub author: String,
    #[darling(default)]
    pub pad_templates: PadTemplates,
    #[darling(default)]
    pub debug_category_colors: Option<PathList>,
}

#[derive(Debug)]
pub struct ElementOptions(pub Attrs);

impl ElementOptions {
    pub fn parse(tokens: TokenStream, errors: &Errors) -> Self {
        if let Err(err) = gst::init() {
            errors.push_spanned(
                &tokens,
                format!("Could not initialized GStreamer {:?}", err),
            );
        }

        Self(util::parse_list(tokens, errors))
    }

    pub fn debug_category_colors(&self) -> TokenStream {
        match self.0.debug_category_colors.as_ref() {
            None => quote!(gst::DebugColorFlags::empty()),
            Some(colors) => {
                quote!(
                    gst::DebugColorFlags::from_bits(
                        #(#colors.bits())*
                    ).unwrap()
                )
            }
        }
    }
}

pub struct ElementDefinition {
    class: ClassDefinition,
    opts: ElementOptions,
}

impl ElementDefinition {
    pub fn parse(
        module: syn::ItemMod,
        opts: ElementOptions,
        crate_path: syn::Path,
        errors: &Errors,
    ) -> Self {
        let mut class = ClassDefinition::parse(
            module,
            ClassOptions(
                opts.0
                    .class
                    .as_ref()
                    .map_or_else(Default::default, |v| v.clone()),
            ),
            crate_path.clone(),
            errors,
        );

        class
            .extends
            .push(syn::parse_quote! { #crate_path::gst::Element });
        class
            .extends
            .push(syn::parse_quote! { #crate_path::gst::Object });

        if class.parent_trait.is_none() {
            class.parent_trait = Some(syn::parse_quote! {
                #crate_path::gst::subclass::prelude::GstObjectImpl
            });
        }
        class.add_private_items();

        let mut res = Self { class, opts };

        res.extend_element(errors);

        res
    }

    fn pad_templates_impl(&self, errors: &Errors) -> Option<TokenStream> {
        if self
            .class
            .inner
            .has_method(TypeMode::Subclass, "pad_templates")
        {
            if !self.opts.0.pad_templates.is_empty() {
                errors.push(Span::mixed_site(),
                    "Using `pad_templates` attribute macro and implementing `ElementImp::pad_templates()` at the same time is not supported."
                );
            }

            Some(quote!(
                fn pad_templates() -> &'static [gst::PadTemplate] {
                    Self::pad_templates()
                }
            ))
        } else {
            let templates = self
                .opts
                .0
                .pad_templates
                .0
                .iter()
                .map(|template| {
                    let name = template.name.as_ref().unwrap();
                    let caps = &template.caps.clone().unwrap_or_else(|| "ANY".to_owned());
                    let direction = match template.direction.as_ref().unwrap() {
                        Direction::Src => quote! {gst::PadDirection::Src },
                        Direction::Sink => quote! { gst::PadDirection::Sink },
                    };
                    let presence = match template.presence.as_ref().unwrap() {
                        Presence::Always => quote! { gst::PadPresence::Always },
                        Presence::Sometimes => quote! { gst::PadPresence::Sometimes },
                        Presence::Request => quote! {gst::PadPresence::Request },
                    };

                    quote!(
                        res.push(gst::PadTemplate::new(
                            #name,
                            #direction,
                            #presence,
                            // Checked at build time
                            &gst::Caps::from_str(#caps).unwrap(),
                        ).unwrap());
                    )
                })
                .collect::<Vec<_>>();

            if !templates.is_empty() {
                Some(quote!(
                    fn pad_templates() -> &'static [gst::PadTemplate] {
                        static PAD_TEMPLATES: Lazy<Vec<gst::PadTemplate>> = Lazy::new(|| {
                            let mut res: Vec<gst::PadTemplate> = Vec::new();
                            #(#templates)*

                            res
                        });

                        PAD_TEMPLATES.as_ref()
                    }
                ))
            } else {
                None
            }
        }
    }

    fn change_state_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "change_state")
            .then(|| {
                quote!(
                    fn change_state(
                        &self,
                        element: &Self::Type,
                        transition: gst::StateChange,
                    ) -> Result<gst::StateChangeSuccess, gst::StateChangeError>
                    {
                        Self::change_state(self, element, transition)
                    }
                )
            })
    }

    fn request_new_pad_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "request_new_pad")
            .then(|| {
                quote!(
                    fn request_new_pad(
                        &self,
                        element: &Self::Type,
                        templ: &gst::PadTemplate,
                        name: Option<String>,
                        caps: Option<&gst::Caps>,
                    ) -> Option<gst::Pad> {
                        Self::request_new_pad(self, element, templ, name, caps)
                    }
                )
            })
    }

    fn release_pad_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "release_pad")
            .then(|| {
                quote!(
                    fn release_pad(&self, element: &Self::Type, pad: &gst::Pad) {
                        Self::release_pad(self, element, pad)
                    }
                )
            })
    }

    fn send_event_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "send_event")
            .then(|| {
                quote!(
                    fn send_event(&self, element: &Self::Type, event: gst::Event) -> bool {
                        self.parent_send_event(self, element, event)
                    }
                )
            })
    }

    fn query_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "query")
            .then(|| {
                quote!(
                    fn query(&self, element: &Self::Type, query: &mut QueryRef) -> bool {
                        Self::query(self, element, query)
                    }
                )
            })
    }

    fn set_context_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "set_context")
            .then(|| {
                quote!(
                    fn set_context(&self, element: &Self::Type, context: &gst::Context) {
                        Self::set_context(self, element, context)
                    }
                )
            })
    }

    fn set_clock_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "set_clock")
            .then(|| {
                quote!(
                    fn set_clock(&self, element: &Self::Type, clock: Option<&gst::Clock>) -> bool {
                        Self::set_clock(self, element, clock)
                    }
                )
            })
    }

    fn provide_clock_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "provide_clock")
            .then(|| {
                quote!(
                    fn provide_clock(&self, element: &Self::Type) -> Option<gst::Clock> {
                        Self::provide_clock(self, element)
                    }
                )
            })
    }

    fn post_message_impl(&self) -> Option<TokenStream> {
        self.class
            .inner
            .has_method(TypeMode::Subclass, "post_message")
            .then(|| {
                quote!(
                    fn post_message(&self, element: &Self::Type, msg: gst::Message) -> bool {
                        Self::post_message(self, element, msg)
                    }
                )
            })
    }

    fn extend_element(&mut self, errors: &Errors) {
        let classname = &self.class.inner.name;
        let long_name = &self.opts.0.long_name;
        let description = &self.opts.0.description;
        let classification = &self.opts.0.classification;
        let author = &self.opts.0.author;
        let templates = self.pad_templates_impl(errors);
        let change_state = self.change_state_impl();
        let request_new_pad = self.request_new_pad_impl();
        let release_pad = self.release_pad_impl();
        let send_event = self.send_event_impl();
        let query = self.query_impl();
        let set_context = self.set_context_impl();
        let set_clock = self.set_clock_impl();
        let provide_clock = self.provide_clock_impl();
        let post_message = self.post_message_impl();
        let factory_name = &self.opts.0.factory_name;
        let debug_colors = &self.opts.debug_category_colors();

        let t: Vec<syn::Item> = vec![syn::Item::Verbatim(quote! {
            use glib::StaticType;
            static CAT: Lazy<gst::DebugCategory> = Lazy::new(|| {
                gst::DebugCategory::new(
                    #factory_name,
                    #debug_colors,
                    Some(#description)
                )
            });

            impl gst::subclass::prelude::GstObjectImpl for #classname {}
            impl gst::subclass::prelude::ElementImpl for #classname {
                fn metadata() -> Option<&'static gst::subclass::ElementMetadata> {
                    static ELEMENT_METADATA: Lazy<gst::subclass::ElementMetadata> = Lazy::new(|| {
                        gst::subclass::ElementMetadata::new(
                            #long_name,
                            #description,
                            #classification,
                            #author,
                        )
                    });

                    Some(&*ELEMENT_METADATA)
                }

                #templates
                #change_state
                #request_new_pad
                #release_pad
                #send_event
                #query
                #set_context
                #set_clock
                #provide_clock
                #post_message
            }
        })];

        self.class.inner.ensure_items().extend(t);
    }
}

impl ToTokens for ElementDefinition {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        let class_token = self.class.to_token_stream();
        let factory_name = &self.opts.0.factory_name;

        let classname = &self.class.inner.name;
        let rank = match self.opts.0.rank.as_ref() {
            // Something went wrong at parse time -> out
            None => return,
            Some(rank) => rank.to_gst_toks(),
        };

        let element_token = quote!(
            #class_token

            pub fn register(plugin: Option<&gst::Plugin>) -> Result<(), glib::BoolError> {
                gst::Element::register(
                    plugin,
                    #factory_name,
                    #rank,
                    #classname::static_type(),
                )
            }
        );

        element_token.to_tokens(tokens);
    }
}
