use glib::{ToVariant, Variant, VariantTy};
use gsk4::prelude::IsRenderNode;
use std::{borrow::Cow, marker::PhantomData};

pub mod render_node {
    use super::*;
    use crate::variant::glib::bytes;
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        bytes::static_variant_type()
    }
    pub fn to_variant(node: &gsk4::RenderNode) -> Variant {
        node.serialize().to_variant()
    }
    pub fn from_variant(variant: &Variant) -> Option<gsk4::RenderNode> {
        gsk4::RenderNode::deserialize(&bytes::from_variant(variant)?)
    }
    declare_optional!(gsk4::RenderNode);
}

pub struct RenderNode<N>(PhantomData<N>);

impl<N: IsRenderNode> RenderNode<N> {
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        render_node::static_variant_type()
    }
    pub fn to_variant(node: &N) -> Variant {
        render_node::to_variant(node.upcast_ref())
    }
    pub fn from_variant(variant: &Variant) -> Option<N> {
        render_node::from_variant(variant)?.downcast().ok()
    }
}

pub struct RenderNodeOptional<N>(PhantomData<N>);

impl<N: IsRenderNode> RenderNodeOptional<N> {
    pub fn static_variant_type() -> Cow<'static, VariantTy> {
        Cow::Borrowed(unsafe { VariantTy::from_str_unchecked("may") })
    }
    pub fn to_variant(value: &Option<N>) -> Variant {
        match value.as_ref() {
            Some(value) => Variant::from_some(&RenderNode::<N>::to_variant(value)),
            None => Variant::from_none(&*RenderNode::<N>::static_variant_type()),
        }
    }
    pub fn from_variant(variant: Variant) -> Option<Option<N>> {
        if !variant.is_type(&*Self::static_variant_type()) {
            return None;
        }
        match variant.as_maybe() {
            Some(variant) => Some(Some(RenderNode::<N>::from_variant(&variant)?)),
            None => Some(None),
        }
    }
}
