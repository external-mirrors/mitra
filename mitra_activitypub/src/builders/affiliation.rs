use apx_sdk::core::url::canonical::NonCanonicalUri;
use serde::Serialize;

use mitra_models::{
    profiles::types::DbActorProfile,
    relationships::types::{
        RelatedActorProfile,
        RelationshipType,
    },
};

use crate::{
    authority::Authority,
    identifiers::{
        local_actor_id_canonical,
        local_relationship_path,
        profile_actor_id,
        IdBuilder,
    },
    vocabulary::RELATIONSHIP,
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Affiliation {
    id: NonCanonicalUri,
    #[serde(rename = "type")]
    object_type: &'static str,
    attributed_to: NonCanonicalUri,
    subject: NonCanonicalUri,
    object: NonCanonicalUri,
    relationship: &'static str,
}

impl Affiliation {
    pub fn new(
        authority: &Authority,
        group: &DbActorProfile,
        related_profile: &RelatedActorProfile<i32>,
    ) -> Self {
        let group_id_builder = IdBuilder::for_profile(authority, group);
        let group_id = local_actor_id_canonical(
            authority.root(),
            group.id,
            &group.username,
        );
        let relationship_path = local_relationship_path(related_profile.related_id);
        let subject_id_builder = IdBuilder::for_profile(
            authority,
            &related_profile.profile,
        );
        let canonical_actor_id = profile_actor_id(
            authority,
            &related_profile.profile,
        );
        Self {
            id: group_id_builder.build_from_path(
                authority.root(),
                relationship_path,
            ),
            object_type: RELATIONSHIP,
            attributed_to: group_id_builder.build(&group_id),
            subject: subject_id_builder.build_unchecked(&canonical_actor_id),
            object: group_id_builder.build(&group_id),
            relationship: match related_profile.relationship_type {
                RelationshipType::GroupAdmin => "admin",
                _ => "none",
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_affiliation() {
        let authority = Authority::server_unchecked("https://social.example");
        let group = DbActorProfile::local_for_test("group");
        let user = DbActorProfile::local_for_test("test");
        let related_user = RelatedActorProfile {
            related_id: 1,
            relationship_type: RelationshipType::GroupAdmin,
            profile: user,
        };
        let affiliation = Affiliation::new(
            &authority,
            &group,
            &related_user,
        );
        let value = serde_json::to_value(affiliation).unwrap();
        let expected_value = serde_json::json!({
            "id": "https://social.example/ap/relationships/1",
            "type": "Relationship",
            "attributedTo": "https://social.example/users/group",
            "subject": "https://social.example/users/test",
            "object": "https://social.example/users/group",
            "relationship": "admin",
        });
        assert_eq!(value, expected_value);
    }
}
