use std::collections::HashMap;

use super::constants::{
    AP_CONTEXT,
    MITRA_CONTEXT,
    W3ID_DATA_INTEGRITY_CONTEXT,
    W3ID_SECURITY_CONTEXT,
};

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
