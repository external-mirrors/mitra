use apx_sdk::{
    constants::AP_PUBLIC,
    core::http_url::HttpUrl,
};
use serde::Serialize;

use mitra_config::Instance;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    profiles::helpers::find_declared_aliases,
    relationships::queries::get_followers,
    users::types::User,
};
use mitra_services::media::MediaServer;
use mitra_utils::id::generate_ulid;

use crate::{
    actors::builders::{build_local_actor, Actor},
    authority::Authority,
    contexts::{build_default_context, Context},
    deliverer::Recipient,
    identifiers::{
        local_activity_id,
        LocalActorCollection,
    },
    queues::OutgoingActivityJobData,
    vocabulary::UPDATE,
};

#[derive(Serialize)]
struct UpdatePerson {
    #[serde(rename = "@context")]
    context: Context,

    #[serde(rename = "type")]
    activity_type: String,

    id: String,
    actor: String,
    object: Actor,

    to: Vec<String>,
    cc: Vec<String>,
}

fn build_update_person(
    instance_url: &HttpUrl,
    media_server: &MediaServer,
    user: &User,
) -> Result<UpdatePerson, DatabaseError> {
    let authority = Authority::server(instance_url);
    let actor = build_local_actor(
        instance_url,
        &authority,
        media_server,
        user,
    )?;
    let followers = LocalActorCollection::Followers.of(&actor.id);
    // Update(Person) is idempotent so its ID can be random
    let activity_id = local_activity_id(
        instance_url.as_str(),
        UPDATE,
        generate_ulid(),
    );
    let activity = UpdatePerson {
        context: build_default_context(),
        activity_type: UPDATE.to_string(),
        id: activity_id,
        actor: actor.id.clone(),
        object: actor,
        to: vec![AP_PUBLIC.to_string()],
        cc: vec![followers],
    };
    Ok(activity)
}

async fn get_update_person_recipients(
    db_client: &impl DatabaseClient,
    user: &User,
) -> Result<Vec<Recipient>, DatabaseError> {
    let followers = get_followers(db_client, user.id).await?;
    let mut recipients = vec![];
    for profile in followers {
        if let Some(remote_actor) = profile.actor_json {
            recipients.extend(Recipient::for_inbox(&remote_actor));
        };
    };
    // Remote aliases
    let aliases = find_declared_aliases(db_client, &user.profile).await?;
    for (_, maybe_profile) in aliases {
        let maybe_remote_actor = maybe_profile
            .and_then(|profile| profile.actor_json);
        if let Some(remote_actor) = maybe_remote_actor {
            recipients.extend(Recipient::for_inbox(&remote_actor));
        };
    };
    Ok(recipients)
}

pub async fn prepare_update_person(
    db_client: &impl DatabaseClient,
    instance: &Instance,
    media_server: &MediaServer,
    user: &User,
) -> Result<OutgoingActivityJobData, DatabaseError> {
    let activity = build_update_person(
        instance.url_ref(),
        media_server,
        user,
    )?;
    let recipients = get_update_person_recipients(db_client, user).await?;
    Ok(OutgoingActivityJobData::new(
        &instance.url(),
        user,
        activity,
        recipients,
    ))
}

#[cfg(test)]
mod tests {
    use mitra_models::profiles::types::DbActorProfile;
    use super::*;

    const INSTANCE_URL: &str = "https://example.com";

    #[test]
    fn test_build_update_person() {
        let instance_url = HttpUrl::parse(INSTANCE_URL).unwrap();
        let media_server = MediaServer::for_test(instance_url.as_str());
        let user = User {
            profile: DbActorProfile::local_for_test("testuser"),
            ..Default::default()
        };
        let activity = build_update_person(
            &instance_url,
            &media_server,
            &user,
        ).unwrap();
        assert_eq!(activity.actor, activity.object.id);
        assert_eq!(
            activity.object.id,
            format!("{}/users/testuser", instance_url),
        );
        assert_eq!(activity.to, vec![AP_PUBLIC.to_string()]);
        assert_eq!(
            activity.cc,
            vec![format!("{}/users/testuser/followers", instance_url)],
        );
    }
}
