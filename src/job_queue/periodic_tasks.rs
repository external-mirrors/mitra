use anyhow::Error;

use mitra_activitypub::queues::{
    process_queued_incoming_activities,
    process_queued_outgoing_activities,
};
use mitra_adapters::media::delete_orphaned_media;
use mitra_config::Config;
use mitra_models::{
    activitypub::queries::delete_activitypub_objects,
    background_jobs::queries::{
        delete_job_from_queue,
        get_job_batch,
    },
    background_jobs::types::JobType,
    database::{get_database_client, DatabaseConnectionPool},
    emojis::queries::{
        delete_emoji,
        find_unused_remote_emojis,
    },
    media::types::DeletionQueue,
    posts::queries::{delete_post, find_extraneous_posts},
    profiles::queries::{
        delete_profile,
        find_empty_profiles,
        get_profile_by_id,
    },
};
use mitra_utils::datetime::days_before_now;

use crate::payments::{
    common::update_expired_subscriptions,
    monero::{check_closed_invoices, check_monero_subscriptions},
};
use super::importer::{
    import_followers_task,
    import_follows_task,
    ImporterJobData,
};

pub async fn subscription_expiration_monitor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    update_expired_subscriptions(
        &config.instance(),
        db_pool,
    ).await?;
    Ok(())
}

pub async fn monero_payment_monitor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    let monero_config = match config.monero_config() {
        Some(monero_config) => monero_config,
        None => return Ok(()), // not configured
    };
    check_monero_subscriptions(
        &config.instance(),
        monero_config,
        db_pool,
    ).await?;
    Ok(())
}

pub async fn monero_recurrent_payment_monitor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    let monero_config = match config.monero_config() {
        Some(monero_config) => monero_config,
        None => return Ok(()), // not configured
    };
    check_closed_invoices(
        monero_config,
        db_pool,
    ).await?;
    Ok(())
}

pub async fn incoming_activity_queue_executor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    process_queued_incoming_activities(config, db_client).await?;
    Ok(())
}

pub async fn outgoing_activity_queue_executor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    process_queued_outgoing_activities(config, db_pool).await?;
    Ok(())
}

pub use mitra_activitypub::queues::fetcher_queue_executor;

pub async fn delete_extraneous_posts(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let updated_before = match config.retention.extraneous_posts {
        Some(days) => days_before_now(days),
        None => return Ok(()), // not configured
    };
    let posts = find_extraneous_posts(db_client, updated_before).await?;
    for post_id in posts {
        let deletion_queue = delete_post(db_client, post_id).await?;
        delete_orphaned_media(config, db_client, deletion_queue).await?;
        log::info!("deleted remote post {}", post_id);
    };
    Ok(())
}

pub async fn delete_empty_profiles(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let updated_before = match config.retention.empty_profiles {
        Some(days) => days_before_now(days),
        None => return Ok(()), // not configured
    };
    let profiles = find_empty_profiles(db_client, updated_before).await?;
    for profile_id in profiles {
        let profile = get_profile_by_id(db_client, profile_id).await?;
        let deletion_queue = delete_profile(db_client, profile.id).await?;
        delete_orphaned_media(config, db_client, deletion_queue).await?;
        log::info!("deleted empty profile {}", profile);
    };
    Ok(())
}

pub async fn prune_remote_emojis(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    let db_client = &mut **get_database_client(db_pool).await?;
    let emojis = find_unused_remote_emojis(db_client).await?;
    for emoji_id in emojis {
        let deletion_queue = delete_emoji(db_client, emoji_id).await?;
        delete_orphaned_media(config, db_client, deletion_queue).await?;
        log::info!("deleted unused emoji {}", emoji_id);
    };
    Ok(())
}

pub async fn prune_activitypub_objects(
    _config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    const CACHE_EXPIRATION_DAYS: u32 = 5;
    let db_client = &**get_database_client(db_pool).await?;
    let created_before = days_before_now(CACHE_EXPIRATION_DAYS);
    let deleted_count =
        delete_activitypub_objects(db_client, created_before).await?;
    if deleted_count > 0 {
        log::info!("deleted {deleted_count} activitypub objects");
    };
    Ok(())
}

pub async fn media_cleanup_queue_executor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    const BATCH_SIZE: u32 = 10;
    const JOB_TIMEOUT: u32 = 600; // 10 minutes
    let db_client = &**get_database_client(db_pool).await?;
    let batch = get_job_batch(
        db_client,
        &JobType::MediaCleanup,
        BATCH_SIZE,
        JOB_TIMEOUT,
    ).await?;
    for job in batch {
        let job_data: DeletionQueue =
            serde_json::from_value(job.job_data)?;
        delete_orphaned_media(config, db_client, job_data).await?;
        delete_job_from_queue(db_client, job.id).await?;
    };
    Ok(())
}

pub async fn importer_queue_executor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), Error> {
    const BATCH_SIZE: u32 = 1;
    const JOB_TIMEOUT: u32 = 6 * 3600; // 6 hours
    let db_client = &mut **get_database_client(db_pool).await?;
    let batch = get_job_batch(
        db_client,
        &JobType::DataImport,
        BATCH_SIZE,
        JOB_TIMEOUT,
    ).await?;
    for job in batch {
        let job_data: ImporterJobData =
            serde_json::from_value(job.job_data)?;
        match job_data {
            ImporterJobData::Follows { user_id, address_list } => {
                import_follows_task(
                    config,
                    db_client,
                    user_id,
                    address_list,
                ).await?;
            },
            ImporterJobData::Followers { user_id, from_actor_id, address_list } => {
                import_followers_task(
                    config,
                    db_client,
                    user_id,
                    from_actor_id,
                    address_list,
                ).await?;
            },
        };
        delete_job_from_queue(db_client, job.id).await?;
    };
    Ok(())
}
