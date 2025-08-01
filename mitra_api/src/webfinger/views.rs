// https://webfinger.net/
use actix_web::{get, web, HttpResponse};
use apx_sdk::{
    addresses::WebfingerAddress,
    core::url::common::Uri,
    jrd::{
        JsonResourceDescriptor,
        Link,
        JRD_MEDIA_TYPE,
    },
};
use serde::Deserialize;

use mitra_activitypub::{
    identifiers::{
        local_actor_id,
        local_instance_actor_id,
        parse_local_actor_id,
    },
    utils::db_url_to_http_url,
};
use mitra_config::{Config, Instance};
use mitra_models::{
    database::{
        get_database_client,
        DatabaseClient,
        DatabaseConnectionPool,
        DatabaseError,
    },
    users::queries::{
        get_portable_user_by_name,
        is_registered_user,
    },
};

use crate::atom::urls::get_user_feed_url;
use crate::errors::HttpError;
use crate::web_client::urls::get_search_page_url;

const WEBFINGER_PROFILE_RELATION_TYPE: &str = "http://webfinger.net/rel/profile-page";
const REMOTE_INTERACTION_RELATION_TYPE: &str = "http://ostatus.org/schema/1.0/subscribe";
// https://codeberg.org/fediverse/fep/src/commit/78a31a92cb264ca603af24b4fcaae944b62edb9b/fep/3b86/fep-3b86.md#5-1-object-intent
const FEP_3B86_OBJECT_INTENT_RELATION_TYPE: &str = "https://w3id.org/fep/3b86/Object";
// Relation type used by Friendica
const FEED_RELATION_TYPE: &str = "http://schemas.google.com/g/2010#updates-from";

async fn get_jrd(
    db_client: &impl DatabaseClient,
    instance: Instance,
    resource: &str,
) -> Result<JsonResourceDescriptor, HttpError> {
    let webfinger_address = if resource.starts_with("acct:") {
        // NOTE: hostname should not contain Unicode characters
        WebfingerAddress::from_acct_uri(resource)
            .map_err(|error| HttpError::ValidationError(error.to_string()))?
    } else {
        // Actor ID? (reverse webfinger)
        let username = if resource == instance.url() ||
            resource == local_instance_actor_id(&instance.url())
        {
            instance.hostname()
        } else {
            parse_local_actor_id(&instance.url(), resource)
                .map_err(|_| HttpError::NotFoundError("user"))?
        };
        WebfingerAddress::new_unchecked(&username, &instance.hostname())
    };
    if webfinger_address.hostname() != instance.hostname() {
        // Wrong instance
        return Err(HttpError::NotFoundError("user"));
    };
    let links = if webfinger_address.username() == instance.hostname() {
        let actor_id = local_instance_actor_id(&instance.url());
        let actor_link = Link::actor(&actor_id);
        // Add remote interaction template
        let remote_interaction_template = get_search_page_url(
            &instance.url(),
            "{uri}",
        );
        let remote_interaction_link = Link::new(REMOTE_INTERACTION_RELATION_TYPE)
            .with_template(&remote_interaction_template);
        vec![actor_link, remote_interaction_link]
    } else if is_registered_user(db_client, webfinger_address.username()).await? {
        let actor_id = local_actor_id(
            &instance.url(),
            webfinger_address.username(),
        );
        // Required by GNU Social
        let profile_link = Link::new(WEBFINGER_PROFILE_RELATION_TYPE)
            .with_media_type("text/html")
            .with_href(&actor_id);
        // Actor link
        let actor_link = Link::actor(&actor_id);
        // Add feed link for users
        let feed_url = get_user_feed_url(
            &instance.url(),
            webfinger_address.username(),
        );
        let feed_link = Link::new(FEED_RELATION_TYPE)
            .with_media_type("application/atom+xml")
            .with_href(&feed_url);
        // Add remote interaction template
        let remote_interaction_template = get_search_page_url(
            &instance.url(),
            "{uri}",
        );
        let remote_interaction_link = Link::new(REMOTE_INTERACTION_RELATION_TYPE)
            .with_template(&remote_interaction_template);
        let fep_3b86_object_intent_template = get_search_page_url(
            &instance.url(),
            "{object}",
        );
        let fep_3b86_object_intent_link = Link::new(FEP_3B86_OBJECT_INTENT_RELATION_TYPE)
            .with_template(&fep_3b86_object_intent_template);
        vec![
            profile_link,
            actor_link,
            feed_link,
            remote_interaction_link,
            fep_3b86_object_intent_link,
        ]
    } else {
        let user = get_portable_user_by_name(
            db_client,
            webfinger_address.username(),
        ).await?;
        let actor_id = user.profile.expect_remote_actor_id();
        let compatible_actor_id = db_url_to_http_url(actor_id, &instance.url())
            .map_err(DatabaseError::from)?;
        let actor_link = Link::actor(&compatible_actor_id);
        vec![actor_link]
    };
    let jrd = JsonResourceDescriptor {
        subject: webfinger_address.to_acct_uri(),
        links: links,
    };
    Ok(jrd)
}

#[derive(Deserialize)]
pub struct WebfingerQueryParams {
    resource: Uri,
}

#[get("/.well-known/webfinger")]
pub async fn webfinger_view(
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    query_params: web::Query<WebfingerQueryParams>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let jrd = get_jrd(
        db_client,
        config.instance(),
        query_params.resource.as_str(),
    ).await?;
    let response = HttpResponse::Ok()
        .content_type(JRD_MEDIA_TYPE)
        .json(jrd);
    Ok(response)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use serial_test::serial;
    use mitra_models::{
        database::test_utils::create_test_database,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_get_jrd() {
        let db_client = &mut create_test_database().await;
        let instance = Instance::for_test("https://social.example");
        let _user = create_test_user(db_client, "test").await;
        let resource = "acct:test@social.example";
        let jrd = get_jrd(db_client, instance, resource).await.unwrap();
        let jrd_value = serde_json::to_value(jrd).unwrap();
        let expected_jrd_value = json!({
            "subject": "acct:test@social.example",
            "links": [
                {
                    "rel": "http://webfinger.net/rel/profile-page",
                    "type": "text/html",
                    "href": "https://social.example/users/test"
                },
                {
                    "rel": "self",
                    "type": "application/ld+json; profile=\"https://www.w3.org/ns/activitystreams\"",
                    "href": "https://social.example/users/test"
                },
                {
                    "rel": "http://schemas.google.com/g/2010#updates-from",
                    "type": "application/atom+xml",
                    "href": "https://social.example/feeds/users/test"
                },
                {
                    "rel": "http://ostatus.org/schema/1.0/subscribe",
                    "template": "https://social.example/search?q={uri}"
                },
                {
                    "rel": "https://w3id.org/fep/3b86/Object",
                    "template": "https://social.example/search?q={object}"
                },
            ]
        });
        assert_eq!(jrd_value, expected_jrd_value);
    }
}
