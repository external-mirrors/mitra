use std::path::PathBuf;

use anyhow::{anyhow, Error};
use apx_sdk::{
    addresses::WebfingerAddress,
    authentication::{
        verify_fetched_object,
        verify_portable_object,
    },
    deliver::{send_object, DelivererError},
    fetch::FetchObjectOptions,
    utils::{get_core_type, CoreType},
};
use clap::{Parser, Subcommand};
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra_activitypub::{
    agent::build_federation_agent,
    authentication::{
        verify_signed_fetched_object,
        verify_signed_object,
    },
    authority::Authority,
    builders::{
        announce::build_relay_announce,
        bite::build_bite,
        like::build_like,
    },
    deliverer::{Recipient, Sender},
    forwarder::get_activity_recipients,
    handlers::activity::get_activity_audience,
    identifiers::canonicalize_id,
    importers::{
        get_post_by_object_id,
        get_user_by_actor_id,
        import_activity,
        import_actor,
        import_collection,
        import_object,
        import_replies,
        ApClient,
        CollectionItemType,
        CollectionOrder,
        FetcherContext,
    },
    webfinger::fetch_webfinger_jrd,
};
use mitra_config::Config;
use mitra_models::{
    accounts::queries::get_user_by_name,
    database::{
        db_client_await,
        get_database_client,
        DatabaseConnectionPool,
    },
    posts::queries::{
        get_post_by_id,
    },
    profiles::queries::get_remote_profile_by_actor_id,
};
use mitra_services::media::MediaServer;
use mitra_utils::id::generate_ulid;

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
                import_actor(&ap_client, db_pool, object).await?;
                println!("profile saved");
            },
            CoreType::Activity => {
                // Process activity
                import_activity(config, &ap_client, db_pool, object).await?;
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
                    &ap_client,
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

/// Load replies from 'replies' or 'context' collection
#[derive(Parser)]
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
    /// Set custom `User-Agent` header
    #[arg(long)]
    user_agent: Option<String>,
    #[arg(long)]
    skip_verification: bool,
    /// Verify FEP-8b32 proof after fetching?
    #[arg(long)]
    verify_proof: bool,
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
        let mut ap_client = ApClient::new_with_pool(config, db_pool).await?;
        ap_client.as_user = maybe_user;
        match self.user_agent.as_deref() {
            Some("") =>
                ap_client.instance.user_agent = None,
            Some(user_agent) =>
                ap_client.instance.user_agent = Some(user_agent.to_owned()),
            None => (), // default
        };
        let gateways = self.gateway
            .map(|gateway| vec![gateway])
            .unwrap_or_default();
        let mut context = FetcherContext::from(gateways);
        let object_id = context.prepare_object_id(&self.object_id)?;
        let options = FetchObjectOptions {
            skip_content_type_verification: self.skip_verification,
        };
        let object = ap_client.fetch_object_raw(
            &object_id,
            options,
        ).await?;
        // TODO: don't verify by default
        if !self.skip_verification {
            if self.verify_proof {
                // Verifies integrity proofs on all objects
                verify_signed_fetched_object(
                    &ap_client,
                    db_pool,
                    &object,
                ).await?;
            } else {
                // Verifies integrity proofs only on portable objects
                verify_fetched_object(&object, vec![])?;
            };
        };
        let object_json = object.extract_fragment()?;
        println!("{}", object_json);
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
        let jrd = fetch_webfinger_jrd(&agent, &webfinger_address).await?;
        println!("{}", jrd);
        Ok(())
    }
}

/// Read portable ActivityPub object from file and process it
#[derive(Parser)]
pub struct LoadPortableObject {
    path: PathBuf,
}

impl LoadPortableObject {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let file_data = std::fs::read(&self.path)?;
        let object_json: JsonValue = serde_json::from_slice(&file_data)?;
        verify_portable_object(&object_json)?;
        let object_class = get_core_type(&object_json);
        let ap_client = ApClient::new_with_pool(config, db_pool).await?;
        match object_class {
            CoreType::Object => {
                import_object(&ap_client, db_pool, object_json).await?;
                println!("object imported");
            },
            CoreType::Actor => {
                import_actor(&ap_client, db_pool, object_json).await?;
                println!("actor imported");
            },
            CoreType::Activity => {
                import_activity(config, &ap_client, db_pool, object_json).await?;
                println!("activity imported");
            },
            _ => return Err(anyhow!("unexpected object class")),
        };
        Ok(())
    }
}

#[derive(Subcommand)]
enum Activity {
    /// mia:Bite activity
    Bite {
        /// Local username
        sender: String,
        /// Actor ID
        recipient: String,
    },
    /// Like activity
    Like {
        /// Local username
        sender: String,
        /// Object ID
        object: String,
    },
    /// LitePub relay Announce activity
    RelayAnnounce {
        /// Internal post ID
        post_id: Uuid,
        /// Actor ID
        recipient: String,
    },
}

/// Create an activity with the specified parameters and print it
#[derive(Parser)]
pub struct CreateActivity {
    #[clap(subcommand)]
    activity: Activity,
}

impl CreateActivity {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let db_client = &**get_database_client(db_pool).await?;
        let instance = config.instance();
        let authority = Authority::from(&instance);
        let activity = match self.activity {
            Activity::Bite { sender, recipient } => {
                let account = get_user_by_name(db_client, &sender).await?;
                let target_profile =
                    get_remote_profile_by_actor_id(db_client, &recipient).await?;
                let bite = build_bite(
                    &authority,
                    &account.profile,
                    target_profile.expect_actor_data(),
                );
                serde_json::to_value(bite)?
            },
            Activity::Like { sender, object } => {
                let account = get_user_by_name(db_client, &sender).await?;
                let canonical_object_id = canonicalize_id(&object)?;
                let post = get_post_by_object_id(
                    db_client,
                    &authority,
                    &canonical_object_id,
                ).await?;
                let media_server = MediaServer::new(config);
                let like = build_like(
                    &authority,
                    &media_server,
                    &account.profile,
                    &post,
                    generate_ulid(),
                    None,
                    None,
                );
                serde_json::to_value(like)?
            },
            Activity::RelayAnnounce { post_id, recipient } => {
                let post = get_post_by_id(db_client, post_id).await?;
                let relay_actor = get_remote_profile_by_actor_id(db_client, &recipient).await?;
                let announce = build_relay_announce(
                    &authority,
                    &post,
                    relay_actor.expect_actor_data(),
                );
                serde_json::to_value(announce)?
            },
        };
        println!("{activity}");
        Ok(())
    }
}

/// Send activity on behalf of a local user.
///
/// Activity will not be verified, and will not be added to the outbox.
#[derive(Parser)]
pub struct SendActivity {
    /// JSON value
    activity: String,
    /// Actor ID
    #[arg(long)]
    recipient: Option<String>,
    /// Create RFC-9421 signature?
    #[arg(long)]
    rfc9421: bool,
}

impl SendActivity {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let instance = config.instance();
        let authority = Authority::from(&instance);
        let activity: JsonValue = serde_json::from_str(&self.activity)?;
        let actor_id = activity["actor"].as_str()
            .ok_or(Error::msg("'actor' property is missing"))?;
        let canonical_actor_id = canonicalize_id(actor_id)?;
        let account = get_user_by_actor_id(
            db_client_await!(db_pool),
            &authority,
            &canonical_actor_id,
        ).await?;

        let recipient = if let Some(recipient) = self.recipient {
            get_remote_profile_by_actor_id(
                db_client_await!(db_pool),
                &recipient,
            ).await?
        } else {
            let audience = get_activity_audience(&activity, None)?;
            let recipients = get_activity_recipients(
                db_client_await!(db_pool),
                &audience,
            ).await?;
            recipients
                .into_iter()
                .next()
                .ok_or(Error::msg("recipient can not be determined"))?
        };
        let recipient_inbox = Recipient
            ::for_inbox(recipient.expect_actor_data())
            .first()
            .ok_or(Error::msg("recipient doesn't have an HTTP inbox"))?
            .inbox
            .clone();
        let sender = Sender::from_user(instance.uri_str(), &account);
        let mut agent = sender.into_agent(&instance);
        if self.rfc9421 {
            agent.rfc9421_enabled = true;
        };
        log::info!("sending activity to {recipient_inbox}");
        match send_object(
            &agent,
            &recipient_inbox,
            &activity,
            &[],
        ).await {
            Ok(response) => {
                println!("{}", response.status);
                if !response.body.is_empty() {
                    println!("{}", response.body);
                };
            },
            Err(error) => {
                println!("{error}");
                if let DelivererError::HttpError(response) = error {
                    if !response.body.is_empty() {
                        println!("{}", response.body);
                    };
                };
            },
        };
        Ok(())
    }
}

/// ActivityPub commands
#[derive(Subcommand)]
pub enum ApCommand {
    Import(ImportObject),
    Fetch(FetchObject),
    Webfinger(Webfinger),
}

impl ApCommand {
    pub async fn execute(
        self,
        config: &Config,
        db_pool: &DatabaseConnectionPool,
    ) -> Result<(), Error> {
        match self {
            Self::Import(command) => command.execute(config, db_pool).await,
            Self::Fetch(command) => command.execute(config, db_pool).await,
            Self::Webfinger(command) => command.execute(config, db_pool).await,
        }
    }
}
