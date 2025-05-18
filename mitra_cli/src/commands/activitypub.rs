use std::path::PathBuf;

use anyhow::{anyhow, Error};
use clap::Parser;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    authentication::verify_portable_object,
    fetch::FetchObjectOptions,
    utils::{get_core_type, CoreType},
};
use mitra_activitypub::{
    agent::build_federation_agent,
    importers::{
        fetch_any_object_with_context,
        import_activity,
        import_collection,
        import_from_outbox,
        import_object,
        import_profile,
        import_replies,
        ApClient,
        CollectionItemType,
        CollectionOrder,
        FetcherContext,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::DatabaseClient,
    users::queries::get_user_by_name,
};

/// Fetch ActivityPub object and process it
#[derive(Parser)]
pub struct ImportObject {
    object_id: String,
    #[arg(long)]
    as_user: Option<String>,
    /// Expected core object type
    #[arg(long, default_value = "any")]
    object_type: String,

    #[arg(long, default_value = "any")]
    collection_type: String,
    #[arg(long, default_value = "forward")]
    collection_order: String,
    #[arg(long, default_value_t = 20)]
    collection_limit: usize,
}

impl ImportObject {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let maybe_user = if let Some(ref username) = self.as_user {
            let user = get_user_by_name(db_client, username).await?;
            Some(user)
        } else {
            None
        };
        let mut ap_client = ApClient::new(config, db_client).await?;
        ap_client.as_user = maybe_user;
        let object: JsonValue =
            ap_client.fetch_object(&self.object_id).await?;
        let object_type = match self.object_type.as_str() {
            "object" => CoreType::Object,
            "actor" => CoreType::Actor,
            "activity" => CoreType::Activity,
            "collection" => CoreType::Collection,
            "any" => get_core_type(&object),
            _ => return Err(anyhow!("invalid object type")),
        };
        match object_type {
            CoreType::Object => {
                // Take contentful object and save it to local cache
                import_object(&ap_client, db_client, object).await?;
                println!("post saved");
            },
            CoreType::Actor => {
                import_profile(&ap_client, db_client, object).await?;
                println!("profile saved");
            },
            CoreType::Activity => {
                // Process activity
                import_activity(config, db_client, object).await?;
                println!("activity processed");
            },
            CoreType::Collection => {
                let maybe_item_type = match self.collection_type.as_str() {
                    "object" => Some(CollectionItemType::Object),
                    "actor" => Some(CollectionItemType::Actor),
                    "activity" => Some(CollectionItemType::Activity),
                    "any" => None,
                    _ => return Err(anyhow!("invalid collection item type")),
                };
                let order = match self.collection_order.as_str() {
                    "forward" => CollectionOrder::Forward,
                    "reverse" => CollectionOrder::Reverse,
                    _ => return Err(anyhow!("invalid collection order type")),
                };
                import_collection(
                    config,
                    db_client,
                    &self.object_id,
                    maybe_item_type,
                    order,
                    self.collection_limit,
                ).await?;
                println!("collection processed");
            },
            _ => return Err(anyhow!("invalid object type")),
        };
        Ok(())
    }
}

/// Pull activities from actor's outbox
#[derive(Parser)]
pub struct ReadOutbox {
    actor_id: String,
    #[arg(long, default_value_t = 20)]
    limit: usize,
}

impl ReadOutbox {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        import_from_outbox(
            config,
            db_client,
            &self.actor_id,
            self.limit,
        ).await?;
        Ok(())
    }
}

/// Load replies from 'replies' or 'context' collection
#[derive(Parser)]
#[command(visible_alias = "fetch-replies")]
pub struct LoadReplies {
    object_id: String,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    use_context: bool,
}

impl LoadReplies {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        import_replies(
            config,
            db_client,
            &self.object_id,
            self.use_context,
            self.limit,
        ).await?;
        Ok(())
    }
}

/// Fetch object as local actor, verify and print it to stdout
#[derive(Parser)]
pub struct FetchObject {
    object_id: String,
    #[arg(long)]
    gateway: Option<String>,
    #[arg(long)]
    as_user: Option<String>,
    #[arg(long)]
    skip_verification: bool,
}

impl FetchObject {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let maybe_user = if let Some(ref username) = self.as_user {
            let user = get_user_by_name(db_client, username).await?;
            Some(user)
        } else {
            None
        };
        let agent = build_federation_agent(
            &config.instance(),
            maybe_user.as_ref(),
        );
        let gateways = self.gateway.as_ref()
            .map(|gateway| vec![gateway.clone()])
            .unwrap_or_default();
        let mut context = FetcherContext::from(gateways);
        let options = FetchObjectOptions {
            skip_verification: self.skip_verification,
            ..Default::default()
        };
        let object: JsonValue = fetch_any_object_with_context(
            &agent,
            &mut context,
            &self.object_id,
            options,
        ).await?;
        println!("{}", object);
        Ok(())
    }
}

#[derive(Parser)]
pub struct LoadPortableObject {
    path: PathBuf,
}

impl LoadPortableObject {
    #[allow(clippy::unused_async)]
    pub async fn execute(
        &self,
        _config: &Config,
        _db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let file_data = std::fs::read(&self.path)?;
        let object_json: JsonValue = serde_json::from_slice(&file_data)?;
        verify_portable_object(&object_json)?;
        println!("portable object is valid");
        Ok(())
    }
}
