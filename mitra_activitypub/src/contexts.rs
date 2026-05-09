use indexmap::IndexMap;
use serde::ser::{Serialize, SerializeSeq, Serializer};

pub use apx_sdk::constants::{
    AP_CONTEXT,
};

pub const W3C_CID_CONTEXT: &str = "https://www.w3.org/ns/cid/v1";
pub const W3ID_SECURITY_CONTEXT: &str = "https://w3id.org/security/v1";
pub const W3ID_DATA_INTEGRITY_CONTEXT: &str = "https://w3id.org/security/data-integrity/v2";
pub const W3ID_VALUEFLOWS_CONTEXT: &str = "https://w3id.org/valueflows/ont/vf#";
pub const SCHEMA_ORG_CONTEXT: &str = "http://schema.org/";
pub const MASTODON_CONTEXT: &str = "http://joinmastodon.org/ns#";
pub const MITRA_CONTEXT: &str = "http://jsonld.mitra.social#";

#[derive(Debug, PartialEq)]
pub struct Context {
    pub vec: Vec<&'static str>,
    // The order does not matter for JSON-LD processors
    // and will not be preserved during the serialization.
    pub map: IndexMap<&'static str, &'static str>,
}

impl Serialize for Context {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
        let mut seq = serializer.serialize_seq(Some(self.vec.len() + 1))?;
        for uri in &self.vec {
            seq.serialize_element(uri)?;
        };
        seq.serialize_element(&self.map)?;
        seq.end()
    }
}

// Default context for activities and objects
pub fn build_default_context() -> Context {
    Context {
        vec: vec![
            AP_CONTEXT,
            W3ID_SECURITY_CONTEXT,
            W3ID_DATA_INTEGRITY_CONTEXT,
        ],
        map: IndexMap::from([
            // Copied from Mastodon
            ("Hashtag", "as:Hashtag"),
            ("sensitive", "as:sensitive"),
            ("toot", MASTODON_CONTEXT),
            ("Emoji", "toot:Emoji"),
        ]),
    }
}
