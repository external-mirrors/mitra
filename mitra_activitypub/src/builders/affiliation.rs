use apx_sdk::core::url::canonical::NonCanonicalUri;
use serde::Serialize;

use mitra_models::relationships::types::{
    RelatedActorProfile,
    RelationshipType,
};

use crate::{
    authority::Authority,
    identifiers::{profile_actor_id, IdBuilder},
    vocabulary::RELATIONSHIP,
};

#[derive(Serialize)]
pub struct Affiliation {
    #[serde(rename = "type")]
    object_type: &'static str,
    subject: NonCanonicalUri,
    relationship: &'static str,
}

impl Affiliation {
    pub fn new(
        authority: &Authority,
        related_profile: &RelatedActorProfile<i32>,
    ) -> Self {
        let id_builder = IdBuilder::for_profile(
            authority,
            &related_profile.profile,
        );
        let canonical_actor_id = profile_actor_id(
            authority,
            &related_profile.profile,
        );
        Self {
            object_type: RELATIONSHIP,
            subject: id_builder.build_unchecked(&canonical_actor_id),
            relationship: match related_profile.relationship_type {
                RelationshipType::GroupAdmin => "admin",
                _ => "none",
            },
        }
    }
}
