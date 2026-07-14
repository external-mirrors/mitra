use apx_sdk::constants::AP_PUBLIC;
use serde::Serialize;
use uuid::Uuid;

use mitra_config::Instance;
use mitra_models::{
    accounts::types::ManagedAccount,
    database::{DatabaseClient, DatabaseError},
    profiles::types::DbActorProfile,
    relationships::queries::{get_followers, get_following},
};

use crate::{
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        local_activity_id_unified,
        local_actor_id_unified,
    },
    queues::OutgoingActivityJobData,
    vocabulary::DELETE,
};

#[derive(Serialize)]
struct DeletePerson {
    #[serde(rename = "@context")]
    _context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: String,

    to: Vec<String>,
}

fn build_delete_person(
    authority: &Authority,
    actor_profile: &DbActorProfile,
) -> DeletePerson {
    let actor_id = local_actor_id_unified(
        authority,
        actor_profile.id,
        &actor_profile.username,
    );
    let activity_id = local_activity_id_unified(
        authority,
        DELETE,
        actor_profile.id,
    );
    DeletePerson {
        _context: build_default_context(),
        activity_type: DELETE.to_string(),
        id: activity_id,
        actor: actor_id.clone(),
        object: actor_id,
        to: vec![AP_PUBLIC.to_string()],
    }
}

#[cfg(not(feature = "mini"))]
async fn get_delete_person_recipients(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
) -> Result<Vec<Recipient>, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let following = get_following(db_client, user_id).await?;
    let mut recipients = vec![];
    for profile in followers.into_iter().chain(following) {
        if let Some(remote_actor) = profile.actor_json {
            recipients.extend(Recipient::for_inbox(&remote_actor));
        };
    };
    Ok(recipients)
}

pub async fn prepare_delete_person(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    account: &impl ManagedAccount,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let authority = Authority::from(instance);
    let activity = build_delete_person(&authority, account.profile());
    #[cfg(not(feature = "mini"))]
    let recipients = get_delete_person_recipients(db_client, account.id()).await?;
    #[cfg(feature = "mini")]
    let recipients = crate::c2s::audience::get_recipients(instance, account);
    Ok(OutgoingActivityJobData::new(
        &authority,
        account,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use apx_sdk::core::url::http_uri::HttpUri;
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_URI: &str = "https://example.com";

    #[test]
    fn test_build_delete_person() {
        let instance_uri = HttpUri::parse(INSTANCE_URI).unwrap();
        let authority = Authority::server(&instance_uri);
        let profile = DbActorProfile::local_for_test("testuser");
        let activity = build_delete_person(&authority, &profile);
        assert_eq!(
            activity.id,
            format!("{}/activities/delete/{}", INSTANCE_URI, profile.id),
        );
        assert_eq!(activity.actor, activity.object);
        assert_eq!(
            activity.object,
            format!("{}/users/testuser", INSTANCE_URI),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC]);
    }
}
