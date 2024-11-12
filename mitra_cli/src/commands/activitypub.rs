use std::path::PathBuf;

use anyhow::Error;
use clap::Parser;
use serde_json::{Value as JsonValue};

use apx_sdk::{
    authentication::verify_portable_object,
    fetch::FetchObjectOptions,
};
use mitra_activitypub::{
    agent::build_federation_agent,
    importers::{
        fetch_any_object_with_context,
        import_activity,
        import_from_outbox,
        import_replies,
        ActorIdResolver,
        FetcherContext,
    },
};
use mitra_config::Config;
use mitra_models::{
    database::DatabaseClient,
    users::queries::get_user_by_name,
};
use mitra_services::media::MediaStorage;

/// (Re-)fetch actor profile by actor ID
#[derive(Parser)]
pub struct FetchActor {
    id: String,
}

impl FetchActor {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let resolver = ActorIdResolver::default()
            .only_remote()
            .force_refetch();
        resolver.resolve(
            db_client,
            &config.instance(),
            &MediaStorage::from(config),
            &self.id,
        ).await?;
        println!("profile saved");
        Ok(())
    }
}

#[derive(Parser)]
pub struct FetchActivity {
    id: String,
}

impl FetchActivity {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        import_activity(config, db_client, &self.id).await?;
        println!("activity imported");
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

/// Fetch replies
#[derive(Parser)]
pub struct FetchReplies {
    object_id: String,
    #[arg(long, default_value_t = 20)]
    limit: usize,
    #[arg(long)]
    use_context: bool,
    #[arg(long)]
    use_container: bool,
}

impl FetchReplies {
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
            self.use_container,
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
            .map(|gateway| vec![gateway.to_string()])
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
