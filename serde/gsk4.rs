use gsk4::prelude::IsRenderNode;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use std::marker::PhantomData;

pub mod render_node {
    use super::*;
    #[derive(Serialize, Deserialize)]
    #[serde(rename = "gsk4::RenderNode")]
    struct RenderNode(#[serde(with = "crate::glib::bytes")] glib::Bytes);
    pub fn serialize<S: Serializer>(n: &gsk4::RenderNode, s: S) -> Result<S::Ok, S::Error> {
        RenderNode(n.serialize()).serialize(s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<gsk4::RenderNode, D::Error> {
        let RenderNode(bytes) = RenderNode::deserialize(d)?;
        let mut error = None;
        gsk4::RenderNode::deserialize_with_error_func(&bytes, |_, _, e| {
            error = Some(e.clone());
        })
        .ok_or_else(|| {
            error
                .map(de::Error::custom)
                .unwrap_or_else(|| de::Error::custom("Unknown error"))
        })
    }
    declare_optional!(gsk4::RenderNode);
}

pub struct RenderNode<N>(PhantomData<N>);

impl<N: IsRenderNode> RenderNode<N> {
    pub fn serialize<S: Serializer>(n: &N, s: S) -> Result<S::Ok, S::Error> {
        render_node::serialize(n.upcast_ref(), s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<N, D::Error> {
        render_node::deserialize(d)?.downcast().map_err(|n| {
            de::Error::custom(format!(
                "Wrong type for RenderNode: Expected {}, got {}",
                N::NODE_TYPE,
                n.node_type()
            ))
        })
    }
}

pub struct RenderNodeOptional<N>(PhantomData<N>);

impl<N: IsRenderNode> RenderNodeOptional<N> {
    pub fn serialize<S: Serializer>(n: &Option<N>, s: S) -> Result<S::Ok, S::Error> {
        #[derive(serde::Serialize)]
        #[serde(transparent)]
        struct Writer<'w, N: IsRenderNode>(#[serde(with = "RenderNode::<N>")] &'w N);
        serde::Serialize::serialize(&n.as_ref().map(Writer), s)
    }
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<N>, D::Error> {
        #[derive(serde::Deserialize)]
        #[serde(transparent)]
        struct Reader<N: IsRenderNode>(#[serde(with = "RenderNode::<N>")] N);
        <Option<Reader<N>> as serde::Deserialize>::deserialize(d).map(|o| o.map(|o| o.0))
    }
}
