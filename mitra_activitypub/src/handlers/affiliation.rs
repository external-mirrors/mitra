use apx_sdk::{
    core::url::canonical::NonCanonicalUri,
};
use serde::Deserialize;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_models::{
    database::DatabaseConnectionPool,
    relationships::types::RelationshipType,
};

use crate::{
    errors::HandlerError,
    importers::{
        ActorIdResolver,
        ApClient,
    },
    ownership::get_object_id,
};

#[derive(Deserialize)]
struct Affiliation {
    subject: NonCanonicalUri,
    relationship: String,
}

pub async fn handle_affiliations(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    items: Vec<JsonValue>,
) -> Result<Vec<(Uuid, RelationshipType)>, HandlerError> {
    let mut affiliations = vec![];
    for item in items {
        let affiliation: Affiliation = serde_json::from_value(item)?;
        let subject = ActorIdResolver::default().resolve(
            ap_client,
            db_pool,
            &affiliation.subject.to_string(),
        ).await?;
        let relationship_type = match affiliation.relationship.as_str() {
            "admin" => RelationshipType::GroupAdmin,
            _ => {
                // Ignore unknown affiliation types
                log::warn!(
                    "unknown affiliation: {}",
                    affiliation.relationship,
                );
                continue;
            },
        };
        affiliations.push((subject.id, relationship_type));
    };
    Ok(affiliations)
}

pub async fn handle_fep_1b12_moderators(
    ap_client: &ApClient,
    db_pool: &DatabaseConnectionPool,
    items: Vec<JsonValue>,
) -> Result<Vec<(Uuid, RelationshipType)>, HandlerError> {
    let mut affiliations = vec![];
    for item in items {
        let item_id = get_object_id(&item)?;
        let subject = ActorIdResolver::default().resolve(
            ap_client,
            db_pool,
            item_id,
        ).await?;
        affiliations.push((subject.id, RelationshipType::GroupAdmin));
    };
    Ok(affiliations)
}
