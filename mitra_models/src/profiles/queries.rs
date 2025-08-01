use apx_core::{
    crypto_eddsa::Ed25519SecretKey,
    did::Did,
    did_pkh::DidPkh,
};
use chrono::{DateTime, Utc};
use uuid::Uuid;

use mitra_utils::{
    currencies::Currency,
    id::generate_ulid,
};

use crate::{
    database::{
        catch_unique_violation,
        query_macro::query,
        DatabaseClient,
        DatabaseError,
        DatabaseTypeError,
    },
    emojis::types::DbEmoji,
    instances::queries::create_instance,
    media::types::{DeletionQueue, PartialMediaInfo},
    relationships::types::RelationshipType,
};

use super::types::{
    get_identity_key,
    Aliases,
    DbActorProfile,
    ExtraFields,
    IdentityProofs,
    PaymentOptions,
    ProfileCreateData,
    ProfileEmojis,
    ProfileUpdateData,
    PublicKeys,
    WebfingerHostname,
};

// Nullifies acct field on conflicting records
// (in case remote actor changes username)
async fn prevent_acct_conflict(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    acct: &str,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        UPDATE actor_profile
        SET acct = NULL
        WHERE id != $1 AND acct = $2 AND actor_json IS NOT NULL
        ",
        &[&profile_id, &acct],
    ).await?;
    Ok(())
}

async fn create_profile_emojis(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    emojis: Vec<Uuid>,
) -> Result<Vec<DbEmoji>, DatabaseError> {
    let emojis_rows = db_client.query(
        "
        INSERT INTO profile_emoji (profile_id, emoji_id)
        SELECT $1, emoji.id FROM emoji WHERE id = ANY($2)
        RETURNING (
            SELECT emoji FROM emoji
            WHERE emoji.id = emoji_id
        )
        ",
        &[&profile_id, &emojis],
    ).await?;
    if emojis_rows.len() != emojis.len() {
        return Err(DatabaseError::NotFound("emoji"));
    };
    let emojis = emojis_rows.iter()
        .map(|row| row.try_get("emoji"))
        .collect::<Result<_, _>>()?;
    Ok(emojis)
}

async fn update_emoji_cache(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
) -> Result<ProfileEmojis, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        WITH profile_emojis AS (
            SELECT
                actor_profile.id AS profile_id,
                COALESCE(
                    jsonb_agg(emoji) FILTER (WHERE emoji.id IS NOT NULL),
                    '[]'
                ) AS emojis
            FROM actor_profile
            LEFT JOIN profile_emoji ON (profile_emoji.profile_id = actor_profile.id)
            LEFT JOIN emoji ON (emoji.id = profile_emoji.emoji_id)
            WHERE actor_profile.id = $1
            GROUP BY actor_profile.id
        )
        UPDATE actor_profile
        SET emojis = profile_emojis.emojis
        FROM profile_emojis
        WHERE actor_profile.id = profile_emojis.profile_id
        RETURNING actor_profile
        ",
        &[&profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile: DbActorProfile = row.try_get("actor_profile")?;
    Ok(profile.emojis)
}

pub(crate) async fn update_emoji_caches(
    db_client: &impl DatabaseClient,
    emoji_id: Uuid,
) -> Result<(), DatabaseError> {
    // TODO: create GIN index on actor_profile.emojis
    db_client.execute(
        "
        WITH profile_emojis AS (
            SELECT
                actor_profile.id AS profile_id,
                COALESCE(
                    jsonb_agg(emoji) FILTER (WHERE emoji.id IS NOT NULL),
                    '[]'
                ) AS emojis
            FROM actor_profile
            LEFT JOIN profile_emoji ON (profile_emoji.profile_id = actor_profile.id)
            LEFT JOIN emoji ON (emoji.id = profile_emoji.emoji_id)
            WHERE actor_profile.emojis @> jsonb_build_array(jsonb_build_object('id', $1::uuid))
            GROUP BY actor_profile.id
        )
        UPDATE actor_profile
        SET emojis = profile_emojis.emojis
        FROM profile_emojis
        WHERE actor_profile.id = profile_emojis.profile_id
        ",
        &[&emoji_id],
    ).await?;
    Ok(())
}

/// Create new profile using given Client or Transaction.
pub async fn create_profile(
    db_client: &mut impl DatabaseClient,
    profile_data: ProfileCreateData,
) -> Result<DbActorProfile, DatabaseError> {
    profile_data.check_consistency()?;
    let transaction = db_client.transaction().await?;
    let profile_id = generate_ulid();
    if let WebfingerHostname::Remote(ref hostname) = profile_data.hostname {
        create_instance(&transaction, hostname).await?;
    };
    let profile_acct = match profile_data.hostname {
        WebfingerHostname::Local => Some(profile_data.username.clone()),
        WebfingerHostname::Remote(ref hostname) => {
            let profile_acct =
                format!("{}@{}", profile_data.username, hostname);
            prevent_acct_conflict(
                &transaction,
                profile_id,
                &profile_acct,
            ).await?;
            Some(profile_acct)
        },
        WebfingerHostname::Unknown => None,
    };
    let row = transaction.query_one(
        "
        INSERT INTO actor_profile (
            id,
            username,
            hostname,
            acct,
            display_name,
            bio,
            avatar,
            banner,
            is_automated,
            manually_approves_followers,
            mention_policy,
            public_keys,
            identity_proofs,
            payment_options,
            extra_fields,
            aliases,
            actor_json
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        RETURNING actor_profile
        ",
        &[
            &profile_id,
            &profile_data.username,
            &profile_data.hostname.as_str(),
            &profile_acct,
            &profile_data.display_name,
            &profile_data.bio,
            &profile_data.avatar,
            &profile_data.banner,
            &profile_data.is_automated,
            &profile_data.manually_approves_followers,
            &profile_data.mention_policy,
            &PublicKeys(profile_data.public_keys),
            &IdentityProofs(profile_data.identity_proofs),
            &PaymentOptions(profile_data.payment_options),
            &ExtraFields(profile_data.extra_fields),
            &Aliases::new(profile_data.aliases),
            &profile_data.actor_json,
        ],
    ).await.map_err(catch_unique_violation("profile"))?;
    let mut profile: DbActorProfile = row.try_get("actor_profile")?;

    // Create related objects
    create_profile_emojis(
        &transaction,
        profile_id,
        profile_data.emojis,
    ).await?;
    profile.emojis = update_emoji_cache(&transaction, profile_id).await?;
    if !profile.is_local() {
        profile.check_consistency()?;
    };
    transaction.commit().await?;
    Ok(profile)
}

pub async fn update_profile(
    db_client: &mut impl DatabaseClient,
    profile_id: Uuid,
    profile_data: ProfileUpdateData,
) -> Result<(DbActorProfile, DeletionQueue), DatabaseError> {
    profile_data.check_consistency()?;
    let transaction = db_client.transaction().await?;
     // Get hostname and currently used images
    let maybe_row = transaction.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile WHERE id = $1
        FOR UPDATE
        ",
        &[&profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile: DbActorProfile = row.try_get("actor_profile")?;
    let detached_files = [&profile.avatar, &profile.banner]
        .into_iter()
        .flatten()
        .filter_map(|image| image.clone().into_file_name())
        .collect();
    if profile_data.hostname.as_str() != profile.hostname.as_deref() &&
        !profile.is_portable()
    {
        // Only portable actors can change hostname
        return Err(DatabaseTypeError.into());
    };
    if let WebfingerHostname::Remote(ref hostname) = profile_data.hostname {
        create_instance(&transaction, hostname).await?;
    };

    let profile_acct = match profile_data.hostname {
        WebfingerHostname::Local => Some(profile_data.username.clone()),
        WebfingerHostname::Remote(ref hostname) => {
            let profile_acct =
                format!("{}@{}", profile_data.username, hostname);
            prevent_acct_conflict(
                &transaction,
                profile_id,
                &profile_acct,
            ).await?;
            Some(profile_acct)
        },
        WebfingerHostname::Unknown => return Err(DatabaseTypeError.into()),
    };
    let maybe_row = transaction.query_opt(
        "
        UPDATE actor_profile
        SET
            username = $1,
            hostname = $2,
            acct = $3,
            display_name = $4,
            bio = $5,
            bio_source = $6,
            avatar = $7,
            banner = $8,
            is_automated = $9,
            manually_approves_followers = $10,
            mention_policy = $11,
            public_keys = $12,
            identity_proofs = $13,
            payment_options = $14,
            extra_fields = $15,
            aliases = $16,
            actor_json = $17,
            updated_at = CURRENT_TIMESTAMP,
            unreachable_since = NULL
        WHERE id = $18
        RETURNING actor_profile
        ",
        &[
            &profile_data.username,
            &profile_data.hostname.as_str(),
            &profile_acct,
            &profile_data.display_name,
            &profile_data.bio,
            &profile_data.bio_source,
            &profile_data.avatar,
            &profile_data.banner,
            &profile_data.is_automated,
            &profile_data.manually_approves_followers,
            &profile_data.mention_policy,
            &PublicKeys(profile_data.public_keys),
            &IdentityProofs(profile_data.identity_proofs),
            &PaymentOptions(profile_data.payment_options),
            &ExtraFields(profile_data.extra_fields),
            &Aliases::new(profile_data.aliases),
            &profile_data.actor_json,
            &profile_id,
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let mut profile: DbActorProfile = row.try_get("actor_profile")?;

    // Delete and re-create related objects
    transaction.execute(
        "DELETE FROM profile_emoji WHERE profile_id = $1",
        &[&profile_id],
    ).await?;
    create_profile_emojis(
        &transaction,
        profile_id,
        profile_data.emojis,
    ).await?;
    profile.emojis = update_emoji_cache(&transaction, profile_id).await?;

    profile.check_consistency()?;
    transaction.commit().await?;

    // Orphaned images should be deleted after update
    let deletion_queue = DeletionQueue {
        files: detached_files,
        ipfs_objects: vec![],
    };
    Ok((profile, deletion_queue))
}

pub async fn set_profile_identity_key(
    db_client: &mut impl DatabaseClient,
    profile_id: Uuid,
    ed25519_secret_key: Ed25519SecretKey,
) -> Result<DbActorProfile, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let identity_key = get_identity_key(ed25519_secret_key);
    let maybe_row = transaction.query_opt(
        "
        UPDATE actor_profile
        SET
            identity_key = $2,
            updated_at = CURRENT_TIMESTAMP
        WHERE id = $1
        RETURNING actor_profile
        ",
        &[&profile_id, &identity_key],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    transaction.commit().await?;
    Ok(profile)
}

pub async fn get_profile_by_id(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE id = $1
        ",
        &[&profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    Ok(profile)
}

pub async fn get_remote_profile_by_actor_id(
    db_client: &impl DatabaseClient,
    actor_id: &str,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE actor_id = $1
        ",
        &[&actor_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    Ok(profile)
}

pub async fn get_profile_by_acct(
    db_client: &impl DatabaseClient,
    acct: &str,
) -> Result<DbActorProfile, DatabaseError> {
    // acct is case-sensitive
    let maybe_row = db_client.query_opt(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE actor_profile.acct = $1
        ",
        &[&acct],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    Ok(profile)
}

pub enum ProfileOrder {
    Active,
    Username,
}

pub async fn get_profiles_paginated(
    db_client: &impl DatabaseClient,
    only_local: bool,
    order: ProfileOrder,
    offset: u16,
    limit: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let mut join = "".to_owned();
    let mut condition = "".to_owned();
    let mut order_by = "".to_owned();
    if only_local {
        condition += "WHERE (user_id IS NOT NULL OR portable_user_id IS NOT NULL)";
    };
    match order {
        ProfileOrder::Active => {
            join += "LEFT JOIN latest_post ON latest_post.author_id = actor_profile.id";
            order_by += "ORDER BY latest_post.created_at DESC NULLS LAST";
        },
        ProfileOrder::Username => {
            order_by += "ORDER BY username ASC";
        },
    };
    let statement = format!(
        "
        SELECT actor_profile
        FROM actor_profile
        {join}
        {condition}
        {order_by}
        LIMIT $1 OFFSET $2
        ",
        join=join,
        condition=condition,
        order_by=order_by,
    );
    let rows = db_client.query(
        &statement,
        &[&i64::from(limit), &i64::from(offset)],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_profiles_by_ids(
    db_client: &impl DatabaseClient,
    profiles_ids: &[Uuid],
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM unnest($1::uuid[]) WITH ORDINALITY AS ranked(id, rank)
        JOIN actor_profile USING (id)
        ORDER BY rank
        ",
        &[&profiles_ids],
    ).await?;
    let profiles: Vec<_> = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    if profiles.len() != profiles_ids.len() {
        return Err(DatabaseError::NotFound("profile"));
    };
    Ok(profiles)
}

pub async fn get_profiles_by_accts(
    db_client: &impl DatabaseClient,
    accts: Vec<String>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM unnest($1::text[]) WITH ORDINALITY AS ranked(acct, rank)
        JOIN actor_profile USING (acct)
        ORDER BY rank
        ",
        &[&accts],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn get_remote_profiles_by_actor_ids(
    db_client: &impl DatabaseClient,
    actors_ids: &[String],
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE actor_id = ANY($1)
        ",
        &[&actors_ids],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

/// Deletes profile from database and returns collection of orphaned objects.
pub async fn delete_profile(
    db_client: &mut impl DatabaseClient,
    profile_id: Uuid,
) -> Result<DeletionQueue, DatabaseError> {
    let transaction = db_client.transaction().await?;
    // Select all posts authored by given actor,
    // their descendants and reposts.
    let posts_rows = transaction.query(
        "
        WITH RECURSIVE context (post_id) AS (
            SELECT post.id FROM post
            WHERE post.author_id = $1
            UNION
            SELECT post.id FROM post
            JOIN context ON (
                post.in_reply_to_id = context.post_id
                OR post.repost_of_id = context.post_id
            )
        )
        SELECT post_id FROM context
        ",
        &[&profile_id],
    ).await?;
    let posts: Vec<Uuid> = posts_rows.iter()
        .map(|row| row.try_get("post_id"))
        .collect::<Result<_, _>>()?;
    // Get list of media files
    let media_rows = transaction.query(
        "
        SELECT unnest(array_remove(
            ARRAY[avatar, banner],
            NULL
        )) AS media
        FROM actor_profile WHERE id = $1
        UNION ALL
        SELECT media
        FROM media_attachment WHERE post_id = ANY($2)
        ",
        &[&profile_id, &posts],
    ).await?;
    let detached_files = media_rows.into_iter()
        .map(|row| row.try_get("media"))
        .collect::<Result<Vec<PartialMediaInfo>, _>>()?
        .into_iter()
        .filter_map(|media| media.into_file_name())
        .collect();
    // Get list of IPFS objects
    let ipfs_objects_rows = transaction.query(
        "
        SELECT ipfs_cid
        FROM media_attachment
        WHERE post_id = ANY($1) AND ipfs_cid IS NOT NULL
        UNION ALL
        SELECT ipfs_cid
        FROM post
        WHERE id = ANY($1) AND ipfs_cid IS NOT NULL
        ",
        &[&posts],
    ).await?;
    let ipfs_objects: Vec<String> = ipfs_objects_rows.iter()
        .map(|row| row.try_get("ipfs_cid"))
        .collect::<Result<_, _>>()?;
    // Update post counters
    transaction.execute(
        "
        UPDATE actor_profile
        SET post_count = post_count - post.count
        FROM (
            SELECT post.author_id, count(*) FROM post
            WHERE post.id = ANY($1)
            GROUP BY post.author_id
        ) AS post
        WHERE actor_profile.id = post.author_id
        ",
        &[&posts],
    ).await?;
    // Update counters
    transaction.execute(
        "
        UPDATE actor_profile
        SET follower_count = follower_count - 1
        FROM relationship
        WHERE
            relationship.source_id = $1
            AND relationship.target_id = actor_profile.id
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
    ).await?;
    transaction.execute(
        "
        UPDATE actor_profile
        SET following_count = following_count - 1
        FROM relationship
        WHERE
            relationship.source_id = actor_profile.id
            AND relationship.target_id = $1
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Follow],
    ).await?;
    transaction.execute(
        "
        UPDATE actor_profile
        SET subscriber_count = subscriber_count - 1
        FROM relationship
        WHERE
            relationship.source_id = $1
            AND relationship.target_id = actor_profile.id
            AND relationship.relationship_type = $2
        ",
        &[&profile_id, &RelationshipType::Subscription],
    ).await?;
    transaction.execute(
        "
        UPDATE post
        SET reply_count = reply_count - reply.count
        FROM (
            SELECT in_reply_to_id, count(*) FROM post
            WHERE author_id = $1 AND in_reply_to_id IS NOT NULL
            GROUP BY in_reply_to_id
        ) AS reply
        WHERE post.id = reply.in_reply_to_id
        ",
        &[&profile_id],
    ).await?;
    transaction.execute(
        "
        UPDATE post
        SET reaction_count = reaction_count - 1
        FROM post_reaction
        WHERE
            post_reaction.post_id = post.id
            AND post_reaction.author_id = $1
        ",
        &[&profile_id],
    ).await?;
    transaction.execute(
        "
        UPDATE post
        SET repost_count = post.repost_count - 1
        FROM post AS repost
        WHERE
            repost.repost_of_id = post.id
            AND repost.author_id = $1
        ",
        &[&profile_id],
    ).await?;
    // Delete profile
    let deleted_count = transaction.execute(
        "
        DELETE FROM actor_profile WHERE id = $1
        RETURNING actor_profile
        ",
        &[&profile_id],
    ).await?;
    if deleted_count == 0 {
        return Err(DatabaseError::NotFound("profile"));
    };
    transaction.commit().await?;
    Ok(DeletionQueue { files: detached_files, ipfs_objects })
}

pub async fn search_profiles(
    db_client: &impl DatabaseClient,
    username: &str,
    maybe_hostname: Option<&String>,
    limit: u16,
    offset: u16,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let db_search_query = match maybe_hostname {
        Some(hostname) => {
            // Search for webfinger address
            format!("{}@{}%", username, hostname)
        },
        None => {
            // Fuzzy search for username
            format!("%{}%", username)
        },
    };
    // Showing local accounts first
    // Showing recently updated profiles first
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE acct ILIKE $1
        ORDER BY
            user_id IS NOT NULL DESC,
            portable_user_id IS NOT NULL DESC,
            updated_at DESC
        LIMIT $2 OFFSET $3
        ",
        &[
            &db_search_query,
            &i64::from(limit),
            &i64::from(offset),
        ],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn search_profiles_by_did_only(
    db_client: &impl DatabaseClient,
    did: &Did,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
     let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE
            EXISTS (
                SELECT 1
                FROM jsonb_array_elements(actor_profile.identity_proofs) AS proof
                WHERE proof ->> 'issuer' = $1
            )
        ",
        &[&did.to_string()],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub async fn search_profiles_by_did(
    db_client: &impl DatabaseClient,
    did: &Did,
    prefer_verified: bool,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let verified = search_profiles_by_did_only(db_client, did).await?;
    let maybe_chain_id_address = match did {
        Did::Pkh(did_pkh) => Some((did_pkh.chain_id(), did_pkh.address())),
        _ => None,
    };
    let unverified = if let Some((chain_id, address)) = maybe_chain_id_address {
        let currency = Currency::from(chain_id);
        // If currency is Ethereum,
        // search over extra fields must be case insensitive.
        let value_op = match currency {
            Currency::Ethereum => "ILIKE",
            Currency::Monero => "LIKE",
        };
        // This query does not scan user_account.wallet_address because
        // login addresses are private.
        let statement = format!(
            "
            SELECT actor_profile
            FROM actor_profile
            WHERE
                EXISTS (
                    SELECT 1
                    FROM jsonb_array_elements(actor_profile.extra_fields) AS field
                    WHERE
                        field ->> 'name' ILIKE $field_name
                        AND field ->> 'value' {value_op} $field_value
                )
            ",
            value_op=value_op,
        );
        let field_name = currency.field_name();
        let query = query!(
            &statement,
            field_name=field_name,
            field_value=address,
        )?;
        let rows = db_client.query(query.sql(), query.parameters()).await?;
        let unverified = rows.iter()
            .map(DbActorProfile::try_from)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            // Exclude verified
            .filter(|profile| !verified.iter().any(|item| item.id == profile.id))
            .collect();
        unverified
    } else {
        vec![]
    };
    let results = if prefer_verified && verified.len() > 0 {
        verified
    } else {
        [verified, unverified].concat()
    };
    Ok(results)
}

pub async fn search_profiles_by_ethereum_address(
    db_client: &impl DatabaseClient,
    wallet_address: &str,
    prefer_verified: bool,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let did_pkh = DidPkh::from_ethereum_address(wallet_address);
    let did = Did::Pkh(did_pkh);
    search_profiles_by_did(db_client, &did, prefer_verified).await
}

pub async fn update_follower_count(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET follower_count = follower_count + $1
        WHERE id = $2
        RETURNING actor_profile
        ",
        &[&change, &profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    Ok(profile)
}

pub async fn update_following_count(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET following_count = following_count + $1
        WHERE id = $2
        RETURNING actor_profile
        ",
        &[&change, &profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    Ok(profile)
}

pub async fn update_subscriber_count(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET subscriber_count = subscriber_count + $1
        WHERE id = $2
        RETURNING actor_profile
        ",
        &[&change, &profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    Ok(profile)
}

pub async fn update_post_count(
    db_client: &impl DatabaseClient,
    profile_id: Uuid,
    change: i32,
) -> Result<DbActorProfile, DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE actor_profile
        SET post_count = post_count + $1
        WHERE id = $2
        RETURNING actor_profile
        ",
        &[&change, &profile_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("profile"))?;
    let profile = DbActorProfile::try_from(&row)?;
    Ok(profile)
}

// Doesn't return error if profile doesn't exist
pub async fn set_reachability_status(
    db_client: &impl DatabaseClient,
    statuses: Vec<(String, bool)>, // (actor_id, is_unreachable)
) -> Result<(), DatabaseError> {
    let statuses_json = serde_json::to_value(statuses)
        .expect("status data should be serializable");
    db_client.execute(
        "
        UPDATE actor_profile
        SET unreachable_since = CASE
            WHEN new.is_unreachable AND unreachable_since IS NOT NULL
                -- don't update if unreachable_since is already set
                THEN unreachable_since
            WHEN new.is_unreachable AND unreachable_since IS NULL
                THEN CURRENT_TIMESTAMP
            ELSE NULL
            END
        FROM (
            SELECT
                (pair ->> 0)::text AS actor_id,
                (pair -> 1)::boolean AS is_unreachable
            FROM jsonb_array_elements($1) AS pair
        ) AS new
        WHERE actor_profile.actor_id = new.actor_id
        ",
        &[&statuses_json],
    ).await?;
    Ok(())
}

pub async fn find_unreachable(
    db_client: &impl DatabaseClient,
    unreachable_since: DateTime<Utc>,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile
        FROM actor_profile
        WHERE unreachable_since < $1
        ORDER BY hostname, username
        ",
        &[&unreachable_since],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

/// Finds all empty remote profiles
/// (without any posts, reactions, relationships)
/// updated before the specified date
pub async fn find_empty_profiles(
    db_client: &impl DatabaseClient,
    updated_before: DateTime<Utc>,
) -> Result<Vec<Uuid>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT actor_profile.id
        FROM actor_profile
        WHERE
            (actor_profile.user_id IS NULL
                AND actor_profile.portable_user_id IS NULL)
            AND actor_profile.updated_at < $1
            AND NOT EXISTS (
                SELECT 1 FROM relationship
                WHERE
                    source_id = actor_profile.id
                    OR target_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM follow_request
                WHERE
                    source_id = actor_profile.id
                    OR target_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM post
                WHERE author_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM post_reaction
                WHERE author_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM media_attachment
                WHERE owner_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM post_mention
                WHERE profile_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM custom_feed_source
                WHERE source_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM notification
                WHERE sender_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM invoice
                WHERE sender_id = actor_profile.id
            )
            AND NOT EXISTS (
                SELECT 1 FROM subscription
                WHERE sender_id = actor_profile.id
            )
        ",
        &[&updated_before],
    ).await?;
    let ids: Vec<Uuid> = rows.iter()
        .map(|row| row.try_get("id"))
        .collect::<Result<_, _>>()?;
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use apx_core::{
        caip2::ChainId,
        crypto_eddsa::generate_weak_ed25519_key,
    };
    use serde_json::json;
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use crate::emojis::{
        queries::create_or_update_local_emoji,
    };
    use crate::media::types::{MediaInfo, PartialMediaInfo};
    use crate::profiles::{
        test_utils::create_test_local_profile,
        types::{
            DbActor,
            DbActorKey,
            ExtraField,
            IdentityProof,
            IdentityProofType,
            PaymentOption,
        },
    };
    use crate::users::{
        queries::create_user,
        test_utils::create_test_portable_user,
        types::UserCreateData,
    };
    use super::*;

    fn create_test_actor(actor_id: &str) -> DbActor {
        DbActor { id: actor_id.to_string(), ..Default::default() }
    }

    #[tokio::test]
    #[serial]
    async fn test_create_profile_local() {
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            payment_options: vec![PaymentOption::monero_subscription(
                ChainId::monero_mainnet(),
                184000000.try_into().unwrap(),
                "testAddress".to_string(),
            )],
            ..Default::default()
        };
        let db_client = &mut create_test_database().await;
        let profile = create_profile(db_client, profile_data).await.unwrap();
        assert_eq!(profile.username, "test");
        assert_eq!(profile.hostname, None);
        assert_eq!(profile.acct.unwrap(), "test");
        assert_eq!(profile.identity_proofs.into_inner().len(), 0);
        assert_eq!(profile.payment_options.inner().len(), 1);
        assert_eq!(profile.extra_fields.into_inner().len(), 0);
        assert_eq!(profile.actor_id, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_profile_remote() {
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: WebfingerHostname::Remote("example.com".to_string()),
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(create_test_actor("https://example.com/users/test")),
            ..Default::default()
        };
        let db_client = &mut create_test_database().await;
        let profile = create_profile(db_client, profile_data).await.unwrap();
        profile.check_consistency().unwrap();
        assert_eq!(profile.username, "test");
        assert_eq!(profile.hostname.unwrap(), "example.com");
        assert_eq!(profile.acct.unwrap(), "test@example.com");
        assert_eq!(
            profile.actor_id.unwrap(),
            "https://example.com/users/test",
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_create_profile_with_emoji() {
        let db_client = &mut create_test_database().await;
        let image = PartialMediaInfo::from(MediaInfo::png_for_test());
        let (emoji, _) = create_or_update_local_emoji(
            db_client,
            "testemoji",
            image,
        ).await.unwrap();
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            emojis: vec![emoji.id.clone()],
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let profile_emojis = profile.emojis.into_inner();
        assert_eq!(profile_emojis.len(), 1);
        assert_eq!(profile_emojis[0].id, emoji.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_actor_id_unique() {
        let db_client = &mut create_test_database().await;
        let actor_id = "https://example.com/users/test";
        let profile_data_1 = ProfileCreateData {
            username: "test-1".to_string(),
            hostname: WebfingerHostname::Remote("example.com".to_string()),
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(create_test_actor(actor_id)),
            ..Default::default()
        };
        create_profile(db_client, profile_data_1).await.unwrap();
        let profile_data_2 = ProfileCreateData {
            username: "test-2".to_string(),
            hostname: WebfingerHostname::Remote("example.com".to_string()),
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(create_test_actor(actor_id)),
            ..Default::default()
        };
        let error = create_profile(db_client, profile_data_2).await.err().unwrap();
        assert_eq!(error.to_string(), "profile already exists");
    }

    #[tokio::test]
    #[serial]
    async fn test_create_profile_acct_conflict() {
        let db_client = &mut create_test_database().await;
        let profile_data_1 = ProfileCreateData {
            username: "test".to_string(),
            hostname: WebfingerHostname::Remote("social.example".to_string()),
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(create_test_actor("https://social.example/users/1")),
            ..Default::default()
        };
        let profile_1 = create_profile(db_client, profile_data_1).await.unwrap();
        assert_eq!(profile_1.acct.unwrap(), "test@social.example");
        let profile_data_2 = ProfileCreateData {
            username: "test".to_string(),
            hostname: WebfingerHostname::Remote("social.example".to_string()),
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(create_test_actor("https://social.example/users/2")),
            ..Default::default()
        };
        let profile_2 = create_profile(db_client, profile_data_2).await.unwrap();
        assert_eq!(profile_2.acct.unwrap(), "test@social.example");

        let profile_1_updated =
            get_profile_by_id(db_client, profile_1.id).await.unwrap();
        assert_eq!(profile_1_updated.acct, None);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_profile() {
        let db_client = &mut create_test_database().await;
        let profile = create_test_local_profile(db_client, "test").await;
        let mut profile_data = ProfileUpdateData::from(&profile);
        let bio = "test bio";
        profile_data.bio = Some(bio.to_string());
        let (profile_updated, deletion_queue) = update_profile(
            db_client,
            profile.id,
            profile_data,
        ).await.unwrap();
        assert_eq!(profile_updated.username, profile.username);
        assert_eq!(profile_updated.acct, profile.acct);
        assert_eq!(profile_updated.bio.unwrap(), bio);
        assert!(profile_updated.updated_at != profile.updated_at);
        assert_eq!(deletion_queue.files.len(), 0);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_profile_with_unmanaged_account() {
        let db_client = &mut create_test_database().await;
        let user = create_test_portable_user(
            db_client,
            "test",
            "ap://did:key:z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6/actor",
        ).await;
        assert_eq!(user.profile.hostname(), WebfingerHostname::Local);
        let mut profile_data = ProfileUpdateData::from(&user.profile);
        let bio = "test bio";
        profile_data.bio = Some(bio.to_string());
        let (profile_updated, _) = update_profile(
            db_client,
            user.id,
            profile_data,
        ).await.unwrap();
        assert_eq!(profile_updated.acct, user.profile.acct);
        assert_eq!(profile_updated.hostname(), user.profile.hostname());
    }

    #[tokio::test]
    #[serial]
    async fn test_set_profile_identity_key() {
        let db_client = &mut create_test_database().await;
        let profile = create_test_local_profile(db_client, "test").await;
        let identity_key = generate_weak_ed25519_key();
        let profile_updated = set_profile_identity_key(
            db_client,
            profile.id,
            identity_key,
        ).await.unwrap();
        assert_eq!(
            profile_updated.identity_key.unwrap(),
            "z6MkvUie7gDQugJmyDQQPhMCCBfKJo7aGvzQYF2BqvFvdwx6",
        );
    }

    #[tokio::test]
    #[serial]
    async fn test_delete_profile() {
        let profile_data = ProfileCreateData::default();
        let db_client = &mut create_test_database().await;
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let deletion_queue = delete_profile(db_client, profile.id).await.unwrap();
        assert_eq!(deletion_queue.files.len(), 0);
        assert_eq!(deletion_queue.ipfs_objects.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_get_profiles_paginated() {
        let db_client = &mut create_test_database().await;
        let profile = create_test_local_profile(db_client, "test").await;
        let profiles = get_profiles_paginated(
            db_client,
            false, // not only local
            ProfileOrder::Active,
            0, // no offset
            40,
        ).await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_profiles() {
        let db_client = &mut create_test_database().await;
        let profile = create_test_local_profile(db_client, "test").await;
        let profiles = search_profiles(
            db_client,
            "tes",
            None,
            10,
            0, // no offset
        ).await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_profiles_by_ethereum_address_local() {
        let db_client = &mut create_test_database().await;
        let wallet_address = "0x1234abcd";
        let user_data = UserCreateData {
            login_address_ethereum: Some(wallet_address.to_string()),
            ..Default::default()
        };
        let _user = create_user(db_client, user_data).await.unwrap();
        let profiles = search_profiles_by_ethereum_address(
            db_client,
            wallet_address,
            false,
        ).await.unwrap();

        // Login address must not be exposed
        assert_eq!(profiles.len(), 0);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_profiles_by_ethereum_address_remote() {
        let db_client = &mut create_test_database().await;
        let extra_field = ExtraField {
            name: "$eth".to_string(),
            value: "0x1234aBcD".to_string(),
            value_source: None,
        };
        let profile_data = ProfileCreateData {
            extra_fields: vec![extra_field],
            ..ProfileCreateData::remote_for_test(
                "test",
                "social.example",
                "https://social.example",
            )
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let profiles = search_profiles_by_ethereum_address(
            db_client,
            "0x1234abcd",
            false,
        ).await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_search_profiles_by_ethereum_address_identity_proof() {
        let db_client = &mut create_test_database().await;
        let identity_proof = IdentityProof {
            issuer: Did::Pkh(DidPkh::from_ethereum_address("0x1234abcd")),
            proof_type: IdentityProofType::LegacyEip191IdentityProof,
            value: json!("13590013185bdea963"),
        };
        let profile_data = ProfileCreateData {
            identity_proofs: vec![identity_proof],
            ..ProfileCreateData::remote_for_test(
                "test",
                "social.example",
                "https://social.example",
            )
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let profiles = search_profiles_by_ethereum_address(
            db_client,
            "0x1234abcd",
            false,
        ).await.unwrap();

        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0].id, profile.id);
    }

    #[tokio::test]
    #[serial]
    async fn test_set_reachability_status() {
        let db_client = &mut create_test_database().await;
        let actor_id = "https://example.com/users/test";
        let profile_data = ProfileCreateData {
            username: "test".to_string(),
            hostname: WebfingerHostname::Remote("example.com".to_string()),
            public_keys: vec![DbActorKey::default()],
            actor_json: Some(create_test_actor(actor_id)),
            ..Default::default()
        };
        let profile = create_profile(db_client, profile_data).await.unwrap();
        let statuses = vec![(actor_id.to_string(), true)];
        set_reachability_status(db_client, statuses).await.unwrap();
        let profile = get_profile_by_id(db_client, profile.id).await.unwrap();
        assert_eq!(profile.unreachable_since.is_some(), true);
    }

    #[tokio::test]
    #[serial]
    async fn test_find_empty_profiles() {
        let db_client = &mut create_test_database().await;
        let updated_before = Utc::now();
        let profiles = find_empty_profiles(db_client, updated_before).await.unwrap();
        assert_eq!(profiles.is_empty(), true);
    }
}
