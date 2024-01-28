use std::collections::HashMap;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use mitra_federation::deserialization::{
    deserialize_string_array,
};

use super::constants::{
    AP_CONTEXT,
    MITRA_CONTEXT,
    W3ID_DATA_INTEGRITY_CONTEXT,
    W3ID_SECURITY_CONTEXT,
};
use super::vocabulary::HASHTAG;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Attachment {
    #[serde(rename = "type")]
    pub attachment_type: String,

    pub name: Option<String>,
    pub media_type: Option<String>,
    pub href: Option<String>,
    pub url: Option<String>,
}

fn default_tag_type() -> String { HASHTAG.to_string() }

#[derive(Deserialize)]
pub struct Tag {
    #[serde(rename = "type", default = "default_tag_type")]
    pub tag_type: String,

    pub name: Option<String>,
    pub href: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SimpleTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub href: String,
    pub name: String,
}

/// https://codeberg.org/silverpill/feps/src/branch/main/e232/fep-e232.md
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub name: Option<String>,
    pub href: String,
    pub media_type: String,
    #[serde(
        default,
        deserialize_with = "deserialize_string_array",
        skip_serializing_if = "Vec::is_empty",
    )]
    pub rel: Vec<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiTagImage {
    #[serde(rename = "type")]
    pub object_type: String,
    pub url: String,
    pub media_type: Option<String>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EmojiTag {
    #[serde(rename = "type")]
    pub tag_type: String,
    pub icon: EmojiTagImage,
    pub id: String,
    pub name: String,
    pub updated: DateTime<Utc>,
}

pub type Context = (
    &'static str,
    &'static str,
    &'static str,
    HashMap<&'static str, &'static str>,
);

pub fn build_default_context() -> Context {
    (
        AP_CONTEXT,
        W3ID_SECURITY_CONTEXT,
        W3ID_DATA_INTEGRITY_CONTEXT,
        HashMap::from([
            ("Hashtag", "as:Hashtag"),
            ("sensitive", "as:sensitive"),
            ("proofValue", "sec:proofValue"),
            ("proofPurpose", "sec:proofPurpose"),
            ("verificationMethod", "sec:verificationMethod"),
            ("mitra", MITRA_CONTEXT),
            ("MitraJcsRsaSignature2022", "mitra:MitraJcsRsaSignature2022"),
        ]),
    )
}
