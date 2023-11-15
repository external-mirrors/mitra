pub use mitra_activitypub::constants::{AP_MEDIA_TYPE, AS_MEDIA_TYPE};

// Contexts
pub const AP_CONTEXT: &str = "https://www.w3.org/ns/activitystreams";
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

// Misc
// https://www.w3.org/TR/activitypub/#public-addressing
pub const AP_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";
