use std::collections::HashMap;

pub use mitra_federation::constants::{
    AP_CONTEXT,
};

pub const W3C_DID_CONTEXT: &str = "https://www.w3.org/ns/did/v1";
pub const W3ID_SECURITY_CONTEXT: &str = "https://w3id.org/security/v1";
pub const W3ID_DATA_INTEGRITY_CONTEXT: &str = "https://w3id.org/security/data-integrity/v1";
pub const W3ID_MULTIKEY_CONTEXT: &str = "https://w3id.org/security/multikey/v1";
pub const W3ID_VALUEFLOWS_CONTEXT: &str = "https://w3id.org/valueflows/ont/vf#";
pub const SCHEMA_ORG_CONTEXT: &str = "http://schema.org/";
pub const MASTODON_CONTEXT: &str = "http://joinmastodon.org/ns#";
pub const MITRA_CONTEXT: &str = "http://jsonld.mitra.social#";
pub const UNITS_OF_MEASURE_CONTEXT: &str = "http://www.ontology-of-units-of-measure.org/resource/om-2/";

pub type Context = (
    &'static str,
    &'static str,
    &'static str,
    HashMap<&'static str, &'static str>,
);

// Default context for activities and objects
pub fn build_default_context() -> Context {
    (
        AP_CONTEXT,
        W3ID_SECURITY_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        HashMap::from([
            ("mitra", MITRA_CONTEXT),
            ("MitraJcsRsaSignature2022", "mitra:MitraJcsRsaSignature2022"),
            // Workarounds for MitraJcsRsaSignature2022
            // (not required for DataIntegrityProof)
            ("proofValue", "sec:proofValue"),
            ("proofPurpose", "sec:proofPurpose"),
            ("verificationMethod", "sec:verificationMethod"),
            // Copied from Mastodon
            ("Hashtag", "as:Hashtag"),
            ("sensitive", "as:sensitive"),
        ]),
    )
}
