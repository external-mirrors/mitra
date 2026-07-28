use apx_sdk::core::url::canonical::NonCanonicalUri;
use serde::Serialize;

use mitra_models::{
    relationships::types::{
        RelatedActorProfile,
        RelationshipType,
    },
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
        let subject_id_builder = IdBuilder::for_profile(
            authority,
            &related_profile.profile,
        );
        let canonical_actor_id = profile_actor_id(
            authority,
            &related_profile.profile,
        );
        Self {
            object_type: RELATIONSHIP,
            subject: subject_id_builder.build_unchecked(&canonical_actor_id),
            relationship: match related_profile.relationship_type {
                RelationshipType::GroupAdmin => "admin",
                _ => "none",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    #[test]
    fn test_build_affiliation() {
        let authority = Authority::server_unchecked("https://social.example");
        let user = DbActorProfile::local_for_test("test");
        let related_user = RelatedActorProfile {
            related_id: 1,
            relationship_type: RelationshipType::GroupAdmin,
            profile: user,
        };
        let affiliation = Affiliation::new(
            &authority,
            &related_user,
        );
        let value = serde_json::to_value(affiliation).unwrap();
        let expected_value = serde_json::json!({
            "type": "Relationship",
            "subject": "https://social.example/users/test",
            "relationship": "admin",
        });
        assert_eq!(value, expected_value);
    }
}
