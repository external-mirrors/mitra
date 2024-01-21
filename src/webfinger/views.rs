/// https://webfinger.net/
use actix_web::{get, web, HttpResponse};
use serde::Deserialize;

use mitra_config::{Config, Instance};
use mitra_federation::{
    addresses::ActorAddress,
    jrd::{
        JsonResourceDescriptor,
        Link,
        JRD_MEDIA_TYPE,
    },
};
use mitra_models::{
    database::{get_database_client, DatabaseClient, DbPool},
    users::queries::is_registered_user,
};

use crate::activitypub::{
    identifiers::{
        local_actor_id,
        local_instance_actor_id,
        parse_local_actor_id,
    },
};
use crate::atom::urls::get_user_feed_url;
use crate::errors::HttpError;

const WEBFINGER_PROFILE_RELATION_TYPE: &str = "http://webfinger.net/rel/profile-page";
// Relation type used by Friendica
const FEED_RELATION_TYPE: &str = "http://schemas.google.com/g/2010#updates-from";

async fn get_jrd(
    db_client: &impl DatabaseClient,
    instance: Instance,
    resource: &str,
) -> Result<JsonResourceDescriptor, HttpError> {
    let actor_address = if resource.starts_with("acct:") {
        ActorAddress::from_acct_uri(resource)
            .map_err(|error| HttpError::ValidationError(error.to_string()))?
    } else {
        // Actor ID? (reverse webfinger)
        let username = if resource == local_instance_actor_id(&instance.url()) {
            instance.hostname()
        } else {
            parse_local_actor_id(&instance.url(), resource)?
        };
        ActorAddress { username, hostname: instance.hostname() }
    };
    if actor_address.hostname != instance.hostname() {
        // Wrong instance
        return Err(HttpError::NotFoundError("user"));
    };
    let actor_id = if actor_address.username == instance.hostname() {
        local_instance_actor_id(&instance.url())
    } else {
        if !is_registered_user(db_client, &actor_address.username).await? {
            return Err(HttpError::NotFoundError("user"));
        };
        local_actor_id(&instance.url(), &actor_address.username)
    };
    // Required by GNU Social
    let profile_link = Link::new(WEBFINGER_PROFILE_RELATION_TYPE)
        .with_media_type("text/html")
        .with_href(&actor_id);
    let actor_link = Link::actor(&actor_id);
    let mut links = vec![profile_link, actor_link];
    if actor_address.username != instance.hostname() {
        // Add feed link for users
        let feed_url = get_user_feed_url(
            &instance.url(),
            &actor_address.username,
        );
        let feed_link = Link::new(FEED_RELATION_TYPE)
            .with_media_type("application/atom+xml")
            .with_href(&feed_url);
        links.push(feed_link);
    };
    let jrd = JsonResourceDescriptor {
        subject: actor_address.to_acct_uri(),
        links: links,
    };
    Ok(jrd)
}

#[derive(Deserialize)]
pub struct WebfingerQueryParams {
    pub resource: String,
}

#[get("/.well-known/webfinger")]
pub async fn webfinger_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    query_params: web::Query<WebfingerQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let jrd = get_jrd(
        db_client,
        config.instance(),
        &query_params.resource,
    ).await?;
    let response = HttpResponse::Ok()
        .content_type(JRD_MEDIA_TYPE)
        .json(jrd);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        users::{
            queries::create_user,
            types::UserCreateData,
        },
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_get_jrd() {
        let db_client = &mut create_test_database().await;
        let instance = Instance::for_test("https://example.com");
        let user_data = UserCreateData {
            username: "test".to_string(),
            password_hash: Some("test".to_string()),
            ..Default::default()
        };
        create_user(db_client, user_data).await.unwrap();
        let resource = "acct:test@example.com";
        let jrd = get_jrd(db_client, instance, resource).await.unwrap();
        assert_eq!(jrd.subject, resource);
        assert_eq!(jrd.links[0].rel, "http://webfinger.net/rel/profile-page");
        assert_eq!(
            jrd.links[0].href.as_ref().unwrap(),
            "https://example.com/users/test",
        );
        assert_eq!(jrd.links[1].rel, "self");
        assert_eq!(
            jrd.links[1].href.as_ref().unwrap(),
            "https://example.com/users/test",
        );
    }
}
