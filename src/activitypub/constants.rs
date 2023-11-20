pub use mitra_activitypub::constants::{
    AP_CONTEXT,
    AP_MEDIA_TYPE,
    AP_PUBLIC,
    AS_MEDIA_TYPE,
};

// Contexts
pub const W3ID_SECURITY_CONTEXT: &str = "https://w3id.org/security/v1";
pub const W3ID_DATA_INTEGRITY_CONTEXT: &str = "https://w3id.org/security/data-integrity/v1";
pub const W3ID_MULTIKEY_CONTEXT: &str = "https://w3id.org/security/multikey/v1";
pub const W3ID_VALUEFLOWS_CONTEXT: &str = "https://w3id.org/valueflows/";
pub const SCHEMA_ORG_CONTEXT: &str = "http://schema.org/";
pub const MASTODON_CONTEXT: &str = "http://joinmastodon.org/ns#";
pub const MITRA_CONTEXT: &str = "http://jsonld.mitra.social#";
pub const UNITS_OF_MEASURE_CONTEXT: &str = "http://www.ontology-of-units-of-measure.org/resource/om-2/";

// Relation types
pub const CHAT_LINK_RELATION_TYPE: &str = "discussion";
pub const PAYMENT_LINK_RELATION_TYPE: &str = "payment";
