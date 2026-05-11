use serde::Serialize;

use mitra_models::{
    profiles::types::{DbActor, DbActorProfile},
};
use mitra_utils::id::generate_ulid;

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    identifiers::{
        expect_compatible_actor_id,
        local_activity_id_unified,
        local_actor_id_unified,
    },
    vocabulary::BITE,
};

// https://ns.mia.jetzt/as/
const BITE_TYPE_IRI: &str = "https://ns.mia.jetzt/as#Bite";

#[derive(Serialize)]
pub struct Bite {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    actor: String,
    id: String,
    target: String,
    to: Vec<String>,
}

pub fn build_bite(
    authority: &Authority,
    actor_profile: &DbActorProfile,
    target_actor: &DbActor,
) -> Bite {
    let mut context = build_default_context();
    context.map.insert(BITE, BITE_TYPE_IRI);
    let actor_id = local_actor_id_unified(
        authority,
        actor_profile.id,
        &actor_profile.username,
    );
    let internal_id = generate_ulid();
    let activity_id = local_activity_id_unified(authority, BITE, internal_id);
    let target_id = expect_compatible_actor_id(target_actor);
    Bite {
        _context: context,
        activity_type: BITE.to_string(),
        actor: actor_id,
        id: activity_id,
        target: target_id.clone(),
        to: vec![target_id],
    }
}
