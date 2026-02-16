use std::path::PathBuf;

use anyhow::{anyhow, Error};
use apx_sdk::{
    addresses::WebfingerAddress,
    authentication::verify_portable_object,
    fetch::{fetch_json, FetchObjectOptions},
    jrd::JRD_MEDIA_TYPE,
    utils::{get_core_type, CoreType},
};
use clap::Parser;
use serde_json::{Value as JsonValue};

use mitra_activitypub::{
    agent::build_federation_agent,
    authentication::verify_signed_object,
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
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
    },
    users::{
        queries::{
            get_user_by_name,
        },
    },
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
    /// Verify FEP-8b32 proof after fetching?
    #[arg(long)]
    verify_proof: bool,
    /// Override fetcher_recursion_limit parameter
    #[arg(long)]
    fetcher_recursion_limit: Option<u16>,

    #[arg(long, default_value = "any")]
    collection_type: String,
    #[arg(long, default_value = "forward")]
    collection_order: String,
    #[arg(long, default_value_t = 20)]
    collection_limit: usize,
}

impl ImportObject {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let maybe_user = if let Some(ref username) = self.as_user {
            let db_client = &**get_database_client(db_pool).await?;
            let user = get_user_by_name(db_client, username).await?;
            Some(user)
        } else {
            None
        };
        let mut ap_client = ApClient::new_with_pool(config, db_pool).await?;
        ap_client.as_user = maybe_user;
        if let Some(limit) = self.fetcher_recursion_limit {
            ap_client.instance.federation.fetcher_recursion_limit = limit;
        };
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
        if self.verify_proof {
            verify_signed_object(
                &ap_client,
                db_pool,
                &object,
                object_type,
                false, // fetch signer
            ).await?;
        };
        match object_type {
            CoreType::Object => {
                // Take contentful object and save it to local cache
                import_object(&ap_client, db_pool, object).await?;
                println!("post saved");
            },
            CoreType::Actor => {
                import_profile(&ap_client, db_pool, object).await?;
                println!("profile saved");
            },
            CoreType::Activity => {
                // Process activity
                import_activity(config, db_pool, object).await?;
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
                    db_pool,
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
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        import_from_outbox(
            config,
            db_pool,
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
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        import_replies(
            config,
            db_pool,
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
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let maybe_user = if let Some(ref username) = self.as_user {
            let user = get_user_by_name(
                db_client_await!(db_pool),
                username,
            ).await?;
            Some(user)
        } else {
            None
        };
        let agent = build_federation_agent(
            &config.instance(),
            maybe_user.as_ref(),
        );
        let gateways = self.gateway
            .map(|gateway| vec![gateway])
            .unwrap_or_default();
        let mut context = FetcherContext::from(gateways);
        let options = FetchObjectOptions {
            skip_verification: self.skip_verification,
            ..Default::default()
        };
        let object = fetch_any_object_with_context(
            &agent,
            &mut context,
            &self.object_id,
            options,
        ).await?;
        println!("{}", object);
        Ok(())
    }
}

/// Perform WebFinger query and print JRD to stdout
#[derive(Parser)]
pub struct Webfinger {
    handle: String,
}

impl Webfinger {
    pub async fn execute(
        self,
        config: &Config,
        _db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let agent = build_federation_agent(&config.instance(), None);
        let webfinger_address = WebfingerAddress::from_handle(&self.handle)?;
        let webfinger_uri = webfinger_address.endpoint_uri();
        let webfinger_resource = webfinger_address.to_acct_uri();
        let jrd = fetch_json(
            &agent,
            &webfinger_uri,
            &[("resource", &webfinger_resource)],
            Some(JRD_MEDIA_TYPE),
        ).await?;
        println!("{}", jrd);
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
        self,
        _config: &Config,
        _db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let file_data = std::fs::read(&self.path)?;
        let object_json: JsonValue = serde_json::from_slice(&file_data)?;
        verify_portable_object(&object_json)?;
        println!("portable object is valid");
        Ok(())
    }
}
