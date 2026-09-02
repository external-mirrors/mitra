use apx_sdk::{
    core::url::canonical::NonCanonicalUri,
    deserialization::deserialize_into_object_id,
};
use serde::{Deserialize, Deserializer, de::Error as DeserializerError};
use serde_json::{Value as JsonValue};

use mitra_models::{
    accounts::queries::get_managed_account_by_id,
    database::{get_database_client, DatabaseConnectionPool},
    profiles::queries::get_remote_profile_by_actor_id,
};
use mitra_validators::errors::ValidationError;

use crate::{
    authority::Authority,
    builders::quote::prepare_accept_quote_request,
    importers::{get_post_by_object_id, ApClient},
};

use super::{Descriptor, HandlerResult};

fn deserialize_object_uri<'de, D>(
    deserializer: D,
) -> Result<NonCanonicalUri, D::Error>
    where D: Deserializer<'de>
{
    let object_id = deserialize_into_object_id(deserializer)?;
    NonCanonicalUri::parse(&object_id).map_err(DeserializerError::custom)
}

#[derive(Deserialize)]
struct QuoteRequest {
    actor: NonCanonicalUri,
    object: NonCanonicalUri,
    #[serde(deserialize_with = "deserialize_object_uri")]
    instrument: NonCanonicalUri,
}

pub async fn handle_quote_request(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    activity: JsonValue,
) -> HandlerResult {
    let request = QuoteRequest::deserialize(&activity)?;
    let authority = Authority::from(&ap_client.instance);
    let object_id = request.object.into_canonical();
    let db_client = &mut **get_database_client(db_pool).await?;
    let quoted_post = get_post_by_object_id(
        db_client,
        &authority,
        &object_id,
    ).await?;
    if !quoted_post.is_local()
        || !quoted_post.is_public()
        || quoted_post.repost_of_id.is_some()
    {
        return Err(ValidationError("unsupported quote target").into());
    };
    let sender = get_managed_account_by_id(
        db_client,
        quoted_post.author.id,
    ).await?;
    let source_id = request.actor.into_canonical().to_string();
    let source = get_remote_profile_by_actor_id(db_client, &source_id).await?;
    let source_actor = source.actor_json.expect("actor data should be present");
    let request_instrument = request.instrument.to_string();
    prepare_accept_quote_request(
        &ap_client.instance,
        &sender,
        &source_actor,
        quoted_post.id,
        &request_instrument,
        activity,
    )?.save_and_enqueue(db_client).await?;
    Ok(Some(Descriptor::object("Object")))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn test_parse_quote_request_with_inlined_instrument() {
        let request: QuoteRequest = serde_json::from_value(json!({
            "id": "https://remote.example/activities/1",
            "actor": "https://remote.example/users/alice",
            "object": "https://social.example/objects/1",
            "instrument": {
                "id": "https://remote.example/objects/2",
                "type": "Note",
            },
        })).unwrap();
        assert_eq!(
            request.instrument.to_string(),
            "https://remote.example/objects/2",
        );
    }
}
