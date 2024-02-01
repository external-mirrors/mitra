use std::path::PathBuf;
use std::str::FromStr;

use anyhow::{anyhow, Error};
use clap::Parser;
use serde_json::{Value as JsonValue};
use uuid::Uuid;

use mitra::activitypub::{
    agent::build_federation_agent,
    builders::delete_note::prepare_delete_note,
    builders::delete_person::prepare_delete_person,
    importers::{
        import_from_outbox,
        import_replies,
        ActorIdResolver,
    },
};
use mitra::adapters::{
    media::{delete_files, delete_media},
    roles::{
        from_default_role,
        role_from_str,
        role_to_str,
        ALLOWED_ROLES,
    },
};
use mitra::payments::monero::{get_payment_address, reopen_invoice};
use mitra_config::Config;
use mitra_federation::fetch::fetch_object;
use mitra_models::{
    attachments::queries::delete_unused_attachments,
    background_jobs::queries::get_job_count,
    background_jobs::types::JobType,
    cleanup::find_orphaned_files,
    database::DatabaseClient,
    emojis::helpers::get_emoji_by_name,
    emojis::queries::{
        create_or_update_local_emoji,
        delete_emoji,
        find_unused_remote_emojis,
        get_emoji_by_name_and_hostname,
    },
    emojis::types::EmojiImage,
    invoices::queries::{get_invoice_by_address, get_invoice_by_id},
    oauth::queries::delete_oauth_tokens,
    posts::queries::{
        delete_post,
        find_extraneous_posts,
        get_post_by_id,
        get_post_count,
    },
    profiles::helpers::get_profile_by_id_or_acct,
    profiles::queries::{
        delete_profile,
        find_empty_profiles,
        find_unreachable,
        get_profile_by_id,
    },
    subscriptions::queries::{
        reset_subscriptions,
        get_active_subscription_count,
        get_expired_subscription_count,
    },
    users::queries::{
        create_invite_code,
        create_user,
        get_invite_codes,
        get_users_admin,
        get_user_count,
        get_user_by_id,
        get_user_by_name,
        set_user_ed25519_private_key,
        set_user_password,
        set_user_role,
    },
    users::types::UserCreateData,
};
use mitra_services::{
    ethereum::{
        signatures::generate_ecdsa_key,
        sync::save_current_block_number,
        utils::key_to_ethereum_address,
    },
    media::MediaStorage,
    monero::{
        wallet::{
            create_monero_signature,
            create_monero_wallet,
            get_active_addresses,
            get_address_count,
            open_monero_wallet,
            verify_monero_signature,
        },
    },
};
use mitra_utils::{
    crypto_eddsa::generate_ed25519_key,
    crypto_rsa::{
        generate_rsa_key,
        rsa_private_key_to_pkcs8_pem,
    },
    datetime::days_before_now,
    files::sniff_media_type,
    passwords::hash_password,
};
use mitra_validators::{
    emojis::{
        validate_emoji_name,
        EMOJI_LOCAL_MAX_SIZE,
        EMOJI_MEDIA_TYPES,
    },
    users::validate_local_username,
};

/// Mitra admin CLI
#[derive(Parser)]
#[command(version)]
pub struct Cli {
    #[clap(subcommand)]
    pub subcmd: SubCommand,
}

#[derive(Parser)]
pub enum SubCommand {
    GenerateRsaKey(GenerateRsaKey),
    GenerateEthereumAddress(GenerateEthereumAddress),

    GenerateInviteCode(GenerateInviteCode),
    ListInviteCodes(ListInviteCodes),
    CreateUser(CreateUser),
    ListUsers(ListUsers),
    AddEd25519Key(AddEd25519Key),
    SetPassword(SetPassword),
    SetRole(SetRole),
    FetchActor(FetchActor),
    ReadOutbox(ReadOutbox),
    FetchReplies(FetchReplies),
    FetchObjectAs(FetchObjectAs),
    DeleteProfile(DeleteProfile),
    DeletePost(DeletePost),
    DeleteEmoji(DeleteEmoji),
    DeleteExtraneousPosts(DeleteExtraneousPosts),
    DeleteUnusedAttachments(DeleteUnusedAttachments),
    DeleteOrphanedFiles(DeleteOrphanedFiles),
    DeleteEmptyProfiles(DeleteEmptyProfiles),
    PruneRemoteEmojis(PruneRemoteEmojis),
    ListUnreachableActors(ListUnreachableActors),
    AddEmoji(AddEmoji),
    ImportEmoji(ImportEmoji),
    UpdateCurrentBlock(UpdateCurrentBlock),
    ResetSubscriptions(ResetSubscriptions),
    CreateMoneroWallet(CreateMoneroWallet),
    CreateMoneroSignature(CreateMoneroSignature),
    VerifyMoneroSignature(VerifyMoneroSignature),
    ReopenInvoice(ReopenInvoice),
    ListActiveAddresses(ListActiveAddresses),
    GetPaymentAddress(GetPaymentAddress),
    InstanceReport(InstanceReport),
}

/// Generate RSA private key
#[derive(Parser)]
pub struct GenerateRsaKey;

impl GenerateRsaKey {
    pub fn execute(&self) -> Result<(), Error> {
        let private_key = generate_rsa_key()?;
        let private_key_pem = rsa_private_key_to_pkcs8_pem(&private_key)?;
        println!("{}", private_key_pem);
        Ok(())
    }
}

/// Generate ethereum address
#[derive(Parser)]
pub struct GenerateEthereumAddress;

impl GenerateEthereumAddress {
    pub fn execute(&self) -> () {
        let private_key = generate_ecdsa_key();
        let address = key_to_ethereum_address(&private_key);
        println!(
            "address {:?}; private key {}",
            address, private_key.display_secret(),
        );
    }
}

/// Generate invite code
#[derive(Parser)]
pub struct GenerateInviteCode {
    note: Option<String>,
}

impl GenerateInviteCode {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let invite_code = create_invite_code(
            db_client,
            self.note.as_deref(),
        ).await?;
        println!("generated invite code: {}", invite_code);
        Ok(())
    }
}

/// List invite codes
#[derive(Parser)]
pub struct ListInviteCodes;

impl ListInviteCodes {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let invite_codes = get_invite_codes(db_client).await?;
        if invite_codes.is_empty() {
            println!("no invite codes found");
            return Ok(());
        };
        for invite_code in invite_codes {
            if let Some(note) = invite_code.note {
                println!("{} ({})", invite_code.code, note);
            } else {
                println!("{}", invite_code.code);
            };
        };
        Ok(())
    }
}

/// Create new user
#[derive(Parser)]
pub struct CreateUser {
    username: String,
    password: String,
    #[arg(value_parser = ALLOWED_ROLES)]
    role: Option<String>,
}

impl CreateUser {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        validate_local_username(&self.username)?;
        let password_hash = hash_password(&self.password)?;
        let rsa_private_key = generate_rsa_key()?;
        let rsa_private_key_pem =
            rsa_private_key_to_pkcs8_pem(&rsa_private_key)?;
        let ed25519_private_key = generate_ed25519_key();
        let role = match &self.role {
            Some(value) => role_from_str(value)?,
            None => from_default_role(&config.registration.default_role),
        };
        let user_data = UserCreateData {
            username: self.username.clone(),
            password_hash: Some(password_hash),
            login_address_ethereum: None,
            login_address_monero: None,
            rsa_private_key: rsa_private_key_pem,
            ed25519_private_key: ed25519_private_key,
            invite_code: None,
            role,
        };
        create_user(db_client, user_data).await?;
        println!("user created");
        Ok(())
    }
}

/// List local users
#[derive(Parser)]
pub struct ListUsers;

impl ListUsers {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let users = get_users_admin(db_client).await?;
        println!(
            "{0: <40} | {1: <35} | {2: <20} | {3: <35} | {4: <35}",
            "ID", "username", "role", "created", "last login",
        );
        for user in users {
            println!(
                "{0: <40} | {1: <35} | {2: <20} | {3: <35} | {4: <35}",
                user.profile.id.to_string(),
                user.profile.username,
                role_to_str(&user.role),
                user.profile.created_at.to_string(),
                user.last_login.map(|dt| dt.to_string()).unwrap_or_default(),
            );
        };
        Ok(())
    }
}

/// Add Ed25519 key to user's profile
#[derive(Parser)]
pub struct AddEd25519Key {
    id: Uuid,
}

impl AddEd25519Key {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let user = get_user_by_id(db_client, &self.id).await?;
        if user.ed25519_private_key.is_some() {
            println!("ed25519 key already exists");
            return Ok(());
        };
        let ed25519_private_key = generate_ed25519_key();
        set_user_ed25519_private_key(
            db_client,
            &self.id,
            ed25519_private_key,
        ).await?;
        println!("ed25519 key generated");
        Ok(())
    }
}

/// Set password
#[derive(Parser)]
pub struct SetPassword {
    id_or_name: String,
    password: String,
}

impl SetPassword {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_id_or_acct(
            db_client,
            &self.id_or_name,
        ).await?;
        let password_hash = hash_password(&self.password)?;
        set_user_password(db_client, &profile.id, &password_hash).await?;
        // Revoke all sessions
        delete_oauth_tokens(db_client, &profile.id).await?;
        println!("password updated");
        Ok(())
    }
}

/// Change user's role
#[derive(Parser)]
pub struct SetRole {
    id_or_name: String,
    #[arg(value_parser = ALLOWED_ROLES)]
    role: String,
}

impl SetRole {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_id_or_acct(
            db_client,
            &self.id_or_name,
        ).await?;
        let role = role_from_str(&self.role)?;
        set_user_role(db_client, &profile.id, role).await?;
        println!("role changed");
        Ok(())
    }
}

/// (Re-)fetch actor profile by actor ID
#[derive(Parser)]
pub struct FetchActor {
    id: String,

    #[arg(long)]
    update_username: bool,
}

impl FetchActor {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let mut resolver = ActorIdResolver::default()
            .only_remote()
            .force_refetch();
        if self.update_username {
            resolver = resolver.update_username();
        };
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
            self.limit,
        ).await?;
        Ok(())
    }
}

/// Fetch object as a local user
#[derive(Parser)]
pub struct FetchObjectAs {
    object_id: String,
    username: String,
}

impl FetchObjectAs {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let user = get_user_by_name(db_client, &self.username).await?;
        let agent = build_federation_agent(&config.instance(), Some(&user));
        let object: JsonValue = fetch_object(&agent, &self.object_id).await?;
        println!("{}", object);
        Ok(())
    }
}

/// Delete profile
#[derive(Parser)]
pub struct DeleteProfile {
    id_or_name: String,
}

impl DeleteProfile {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_id_or_acct(
            db_client,
            &self.id_or_name,
        ).await?;
        let mut maybe_delete_person = None;
        if profile.is_local() {
            let user = get_user_by_id(db_client, &profile.id).await?;
            let activity =
                prepare_delete_person(db_client, &config.instance(), &user).await?;
            maybe_delete_person = Some(activity);
        };
        let deletion_queue = delete_profile(db_client, &profile.id).await?;
        delete_media(config, deletion_queue).await;
        // Send Delete(Person) activities
        if let Some(activity) = maybe_delete_person {
            activity.enqueue(db_client).await?;
        };
        println!("profile deleted");
        Ok(())
    }
}

/// Delete post
#[derive(Parser)]
pub struct DeletePost {
    id: Uuid,
}

impl DeletePost {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let post = get_post_by_id(db_client, &self.id).await?;
        let mut maybe_delete_note = None;
        if post.author.is_local() {
            let author = get_user_by_id(db_client, &post.author.id).await?;
            let activity = prepare_delete_note(
                db_client,
                &config.instance(),
                &author,
                &post,
                config.federation.fep_e232_enabled,
            ).await?;
            maybe_delete_note = Some(activity);
        };
        let deletion_queue = delete_post(db_client, &post.id).await?;
        delete_media(config, deletion_queue).await;
        // Send Delete(Note) activity
        if let Some(activity) = maybe_delete_note {
            activity.enqueue(db_client).await?;
        };
        println!("post deleted");
        Ok(())
    }
}

/// Delete custom emoji
#[derive(Parser)]
pub struct DeleteEmoji {
    emoji_name: String,
    hostname: Option<String>,
}

impl DeleteEmoji {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let emoji = get_emoji_by_name(
            db_client,
            &self.emoji_name,
            self.hostname.as_deref(),
        ).await?;
        let deletion_queue = delete_emoji(db_client, &emoji.id).await?;
        delete_media(config, deletion_queue).await;
        println!("emoji deleted");
        Ok(())
    }
}

/// Delete old remote posts
#[derive(Parser)]
pub struct DeleteExtraneousPosts {
    days: u32,
}

impl DeleteExtraneousPosts {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let updated_before = days_before_now(self.days);
        let posts = find_extraneous_posts(db_client, &updated_before).await?;
        for post_id in posts {
            let deletion_queue = delete_post(db_client, &post_id).await?;
            delete_media(config, deletion_queue).await;
            println!("post {} deleted", post_id);
        };
        Ok(())
    }
}

/// Delete attachments that don't belong to any post
#[derive(Parser)]
pub struct DeleteUnusedAttachments {
    days: u32,
}

impl DeleteUnusedAttachments {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let created_before = days_before_now(self.days);
        let deletion_queue = delete_unused_attachments(
            db_client,
            &created_before,
        ).await?;
        delete_media(config, deletion_queue).await;
        println!("unused attachments deleted");
        Ok(())
    }
}

/// Find and delete orphaned files
#[derive(Parser)]
pub struct DeleteOrphanedFiles;

impl DeleteOrphanedFiles {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let media_storage = MediaStorage::from(config);
        let mut files = vec![];
        for maybe_path in std::fs::read_dir(&media_storage.media_dir)? {
            let file_name = maybe_path?.file_name()
                .to_string_lossy().to_string();
            files.push(file_name);
        };
        println!("found {} files", files.len());
        let orphaned = find_orphaned_files(db_client, files).await?;
        if !orphaned.is_empty() {
            delete_files(&media_storage, orphaned);
            println!("orphaned files deleted");
        };
        Ok(())
    }
}

/// Delete empty remote profiles
#[derive(Parser)]
pub struct DeleteEmptyProfiles {
    days: u32,
}

impl DeleteEmptyProfiles {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let updated_before = days_before_now(self.days);
        let profiles = find_empty_profiles(db_client, &updated_before).await?;
        for profile_id in profiles {
            let profile = get_profile_by_id(db_client, &profile_id).await?;
            let deletion_queue = delete_profile(db_client, &profile.id).await?;
            delete_media(config, deletion_queue).await;
            println!("profile {} deleted", profile.acct);
        };
        Ok(())
    }
}

/// Delete unused remote emojis
#[derive(Parser)]
pub struct PruneRemoteEmojis;

impl PruneRemoteEmojis {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let emojis = find_unused_remote_emojis(db_client).await?;
        for emoji_id in emojis {
            let deletion_queue = delete_emoji(db_client, &emoji_id).await?;
            delete_media(config, deletion_queue).await;
            println!("emoji {} deleted", emoji_id);
        };
        Ok(())
    }
}

/// List unreachable actors
#[derive(Parser)]
pub struct ListUnreachableActors {
    days: u32,
}

impl ListUnreachableActors {
    pub async fn execute(
        &self,
        _config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let unreachable_since = days_before_now(self.days);
        let profiles = find_unreachable(db_client, &unreachable_since).await?;
        println!(
            "{0: <60} | {1: <35} | {2: <35}",
            "ID", "unreachable since", "updated at",
        );
        for profile in profiles {
            println!(
                "{0: <60} | {1: <35} | {2: <35}",
                profile.actor_id
                    .expect("actor ID should be present"),
                profile.unreachable_since
                    .expect("unreachable flag should be present")
                    .to_string(),
                profile.updated_at.to_string(),
            );
        };
        Ok(())
    }
}

/// Add custom emoji to local collection
#[derive(Parser)]
pub struct AddEmoji {
    name: String,
    path: PathBuf,
}

impl AddEmoji {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        if validate_emoji_name(&self.name).is_err() {
            println!("invalid emoji name");
            return Ok(());
        };
        let file = std::fs::read(&self.path)?;
        let media_type = sniff_media_type(&file)
            .ok_or(anyhow!("unknown media type"))?;
        if !EMOJI_MEDIA_TYPES.contains(&media_type.as_str()) {
            println!("media type {} is not supported", media_type);
            return Ok(());
        };
        let file_size = file.len();
        if file_size > EMOJI_LOCAL_MAX_SIZE {
            println!("emoji is too big");
            return Ok(());
        };
        let file_name = MediaStorage::from(config)
            .save_file(file, &media_type)?;
        let image = EmojiImage { file_name, file_size, media_type };
        create_or_update_local_emoji(
            db_client,
            &self.name,
            image,
        ).await?;
        println!("added emoji to local collection");
        Ok(())
    }
}

/// Import custom emoji from another instance
#[derive(Parser)]
pub struct ImportEmoji {
    emoji_name: String,
    hostname: String,
}

impl ImportEmoji {
    pub async fn execute(
        &self,
        _config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let emoji = get_emoji_by_name_and_hostname(
            db_client,
            &self.emoji_name,
            &self.hostname,
        ).await?;
        if emoji.image.file_size > EMOJI_LOCAL_MAX_SIZE {
            println!("emoji is too big");
            return Ok(());
        };
        create_or_update_local_emoji(
            db_client,
            &emoji.emoji_name,
            emoji.image,
        ).await?;
        println!("added emoji to local collection");
        Ok(())
    }
}

/// Update blockchain synchronization starting block
#[derive(Parser)]
pub struct UpdateCurrentBlock {
    number: u64,
}

impl UpdateCurrentBlock {
    pub async fn execute(
        &self,
        _config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        save_current_block_number(db_client, self.number).await?;
        println!("current block updated");
        Ok(())
    }
}

/// Reset all subscriptions
/// (can be used during development or when switching between chains)
#[derive(Parser)]
pub struct ResetSubscriptions {
    // Subscription options are removed by default
    #[arg(long)]
    keep_subscription_options: bool,
}

impl ResetSubscriptions {
    pub async fn execute(
        &self,
        _config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        reset_subscriptions(db_client, self.keep_subscription_options).await?;
        println!("subscriptions deleted");
        Ok(())
    }
}

/// Create Monero wallet
/// (can be used when monero-wallet-rpc runs with --wallet-dir option)
#[derive(Parser)]
pub struct CreateMoneroWallet {
    name: String,
    password: Option<String>,
}

impl CreateMoneroWallet {
    pub async fn execute(
        &self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        create_monero_wallet(
            monero_config,
            self.name.clone(),
            self.password.clone(),
        ).await?;
        println!("wallet created");
        Ok(())
    }
}

/// Create Monero signature
#[derive(Parser)]
pub struct CreateMoneroSignature {
    message: String,
}

impl CreateMoneroSignature {
    pub async fn execute(
        &self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let (address, signature) =
            create_monero_signature(monero_config, &self.message).await?;
        println!("address: {}", address);
        println!("signature: {}", signature);
        Ok(())
    }
}

/// Verify Monero signature
#[derive(Parser)]
pub struct VerifyMoneroSignature {
    address: String,
    message: String,
    signature: String,
}

impl VerifyMoneroSignature {
    pub async fn execute(
        &self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        verify_monero_signature(
            monero_config,
            &self.address,
            &self.message,
            &self.signature,
        ).await?;
        println!("signature verified");
        Ok(())
    }
}

/// Re-open closed invoice (already processed, timed out or cancelled)
#[derive(Parser)]
pub struct ReopenInvoice {
    id_or_address: String,
}

impl ReopenInvoice {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let invoice = if let Ok(invoice_id) = Uuid::from_str(&self.id_or_address) {
            get_invoice_by_id(db_client, &invoice_id).await?
        } else {
            get_invoice_by_address(
                db_client,
                &monero_config.chain_id,
                &self.id_or_address,
            ).await?
        };
        reopen_invoice(
            monero_config,
            db_client,
            &invoice,
        ).await?;
        Ok(())
    }
}

#[derive(Parser)]
pub struct ListActiveAddresses;

impl ListActiveAddresses {
    pub async fn execute(
        &self,
        config: &Config,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let wallet_client = open_monero_wallet(monero_config).await?;
        let addresses = get_active_addresses(
            &wallet_client,
            monero_config.account_index,
        ).await?;
        for (address, amount) in addresses {
            println!("{}: {}", address, amount);
        };
        Ok(())
    }
}

/// Get payment address for given sender and recipient
#[derive(Parser)]
pub struct GetPaymentAddress {
    sender_id: Uuid,
    recipient_id: Uuid,
}

impl GetPaymentAddress {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &mut impl DatabaseClient,
    ) -> Result<(), Error> {
        let monero_config = config.monero_config()
            .ok_or(anyhow!("monero configuration not found"))?;
        let payment_address = get_payment_address(
            monero_config,
            db_client,
            &self.sender_id,
            &self.recipient_id,
        ).await?;
        print!("payment address: {}", payment_address);
        Ok(())
    }
}

/// Display instance report
#[derive(Parser)]
pub struct InstanceReport;

impl InstanceReport {
    pub async fn execute(
        &self,
        config: &Config,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        // General info
        let users = get_user_count(db_client).await?;
        let incoming_activities =
            get_job_count(db_client, JobType::IncomingActivity).await?;
        let outgoing_activities =
            get_job_count(db_client, JobType::OutgoingActivity).await?;
        let posts = get_post_count(db_client, false).await?;
        println!("local users: {}", users);
        println!("incoming activities: {}", incoming_activities);
        println!("outgoing activities: {}", outgoing_activities);
        println!("total posts: {}", posts);
        // Subscriptions
        let active_subscriptions =
            get_active_subscription_count(db_client).await?;
        let expired_subscriptions =
            get_expired_subscription_count(db_client).await?;
        println!("active subscriptions: {}", active_subscriptions);
        println!("expired subscriptions: {}", expired_subscriptions);
        if let Some(monero_config) = config.monero_config() {
            let wallet_client = open_monero_wallet(monero_config).await?;
            let address_count = get_address_count(
                &wallet_client,
                monero_config.account_index,
            ).await?;
            println!("monero addresses: {}", address_count);
        };
        Ok(())
    }
}


#[cfg(test)]
mod tests {
    use clap::CommandFactory;
    use super::*;

    #[test]
    fn test_cli() {
        Cli::command().debug_assert()
    }
}
