use std::collections::BTreeMap;
use std::time::{Duration as StdDuration, Instant};

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value as JsonValue};

use apx_core::http_url::HttpUrl;
use apx_sdk::{
    fetch::FetchError,
};
use mitra_config::Config;
use mitra_models::{
    activitypub::queries::{
        save_activity,
        add_object_to_collection,
    },
    background_jobs::queries::{
        enqueue_job,
        get_job_batch,
        delete_job_from_queue,
    },
    background_jobs::types::JobType,
    database::{
        get_database_client,
        DatabaseClient,
        DatabaseConnectionPool,
        DatabaseError,
        DatabaseTypeError,
    },
    filter_rules::types::FilterAction,
    profiles::queries::{
        get_remote_profile_by_actor_id,
        set_reachability_status,
    },
    profiles::types::DbActor,
    users::types::{PortableUser, User},
};

use crate::{
    deliverer::{
        deliver_activity_worker,
        sign_activity,
        Recipient,
        Sender,
    },
    errors::HandlerError,
    filter::FederationFilter,
    handlers::activity::handle_activity,
    identifiers::canonicalize_id,
    importers::{
        import_featured,
        import_from_outbox,
        import_replies,
        is_actor_importer_error,
    },
    utils::{db_url_to_http_url, parse_http_url_from_db},
};

const JOB_TIMEOUT: u32 = 3600; // 1 hour

#[derive(Deserialize, Serialize)]
pub struct IncomingActivityJobData {
    activity: JsonValue,
    is_authenticated: bool,
    failure_count: u32,
}

impl IncomingActivityJobData {
    pub fn new(activity: &JsonValue, is_authenticated: bool) -> Self {
        Self {
            activity: activity.clone(),
            is_authenticated,
            failure_count: 0,
        }
    }

    pub async fn into_job(
        self,
        db_client: &impl DatabaseClient,
        delay: u32,
    ) -> Result<(), DatabaseError> {
        let job_data = serde_json::to_value(self)
            .expect("activity should be serializable");
        let scheduled_for = Utc::now() + Duration::seconds(delay.into());
        enqueue_job(
            db_client,
            JobType::IncomingActivity,
            &job_data,
            scheduled_for,
        ).await
    }
}

const INCOMING_QUEUE_RETRIES_MAX: u32 = 2;

const fn incoming_queue_backoff(_failure_count: u32) -> u32 {
    // Constant, 10 minutes
    60 * 10
}

pub async fn process_queued_incoming_activities(
    config: &Config,
    db_client: &mut impl DatabaseClient,
) -> Result<(), DatabaseError> {
    let batch = get_job_batch(
        db_client,
        JobType::IncomingActivity,
        config.federation.inbox_queue_batch_size,
        JOB_TIMEOUT,
    ).await?;
    for job in batch {
        let mut job_data: IncomingActivityJobData =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        let duration_max =
            StdDuration::from_secs((JOB_TIMEOUT / 6).into());
        let handler_future = handle_activity(
            config,
            db_client,
            &job_data.activity,
            job_data.is_authenticated,
            false, // activity was pushed
        );
        let handler_result = match tokio::time::timeout(
            duration_max,
            handler_future,
        ).await {
            Ok(result) => result,
            Err(_) => {
                log::error!(
                    "failed to process activity (timeout): {}",
                    job_data.activity,
                );
                delete_job_from_queue(db_client, job.id).await?;
                continue;
            },
        };
        if let Err(error) = handler_result {
            if !matches!(
                error,
                HandlerError::FetchError(FetchError::RequestError(_))
            ) {
                // Error is not retriable
                log::warn!(
                    "failed to process activity ({}): {}",
                    error,
                    job_data.activity,
                );
                delete_job_from_queue(db_client, job.id).await?;
                continue;
            };
            job_data.failure_count += 1;
            log::warn!(
                "failed to process activity ({}) (attempt #{}): {}",
                error,
                job_data.failure_count,
                job_data.activity,
            );
            if job_data.failure_count <= INCOMING_QUEUE_RETRIES_MAX {
                // Re-queue
                let retry_after = incoming_queue_backoff(job_data.failure_count);
                job_data.into_job(db_client, retry_after).await?;
                log::info!("activity re-queued");
            };
        };
        delete_job_from_queue(db_client, job.id).await?;
    };
    Ok(())
}

#[derive(Deserialize, Serialize)]
pub struct OutgoingActivityJobData {
    activity: JsonValue,
    sender: Sender,
    recipients: Vec<Recipient>,
    failure_count: u32,
}

impl OutgoingActivityJobData {
    fn prepare_recipients(
        instance_url: &str,
        actors: Vec<DbActor>,
    ) -> Vec<Recipient> {
        let mut recipients = vec![];
        for actor in actors {
            recipients.extend(Recipient::from_actor_data(&actor));
        };
        Self::mark_local_recipients(instance_url, &mut recipients);
        recipients
    }

    fn mark_local_recipients(
        instance_url: &str,
        recipients: &mut [Recipient],
    ) -> () {
        // If portable actor has local account,
        // activity will be simply added to its inbox
        let instance_url = HttpUrl::parse(instance_url)
            .expect("instance URL should be valid");
        for recipient in recipients.iter_mut() {
            let recipient_inbox = parse_http_url_from_db(&recipient.inbox)
                .expect("actor inbox URL should be valid");
            recipient.is_local = recipient_inbox.origin() == instance_url.origin();
        };
    }

    fn sort_recipients(mut recipients: Vec<Recipient>) -> Vec<Recipient> {
        // De-duplicate recipients.
        // Keys are inboxes, not actor IDs, because one actor
        // can have multiple inboxes.
        recipients.sort_by_key(|recipient| {
            (recipient.inbox.clone(), !recipient.is_primary)
        });
        recipients.dedup_by_key(|recipient| recipient.inbox.clone());
        // Sort recipients
        recipients.sort_by_key(|recipient| {
            // Primary recipients are first
            (!recipient.is_primary, recipient.inbox.clone())
        });
        recipients
    }

    pub(super) fn new(
        instance_url: &str,
        sender: &User,
        activity: impl Serialize,
        mut recipients: Vec<Recipient>,
    ) -> Self {
        Self::mark_local_recipients(instance_url, &mut recipients);
        let recipients = Self::sort_recipients(recipients);
        let activity = serde_json::to_value(activity)
            .expect("activity should be serializable");
        let activity_signed = sign_activity(
            instance_url,
            sender,
            activity,
        ).expect("activity should be valid");
        Self {
            activity: activity_signed,
            sender: Sender::from_user(instance_url, sender),
            recipients: recipients,
            failure_count: 0,
        }
    }

    pub fn new_forwarded(
        instance_url: &str,
        sender: &PortableUser,
        activity: &JsonValue,
        recipients: Vec<DbActor>,
    ) -> Option<Self> {
        let mut recipients = Self::prepare_recipients(instance_url, recipients);
        let actor_data = sender.profile.expect_actor_data();
        // Deliver to actor's clones
        for gateway_url in &actor_data.gateways {
            if gateway_url == instance_url {
                // Already cached
                continue;
            };
            let http_actor_outbox = db_url_to_http_url(&actor_data.outbox, gateway_url)
                .expect("actor outbox URL should be valid");
            let recipient = Recipient::new(&actor_data.id, &http_actor_outbox);
            recipients.push(recipient);
        };
        let recipients = Self::sort_recipients(recipients);
        let sender = Sender::from_portable_user(instance_url, sender)?;
        let job_data = Self {
            activity: activity.clone(),
            sender: sender,
            recipients: recipients,
            failure_count: 0,
        };
        Some(job_data)
    }

    pub fn activity(&self) -> &JsonValue {
        &self.activity
    }

    async fn save_activity(
        &mut self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        // Activity ID should be present
        let activity_id = self.activity["id"].as_str()
            .ok_or(DatabaseTypeError)?;
        let canonical_activity_id = canonicalize_id(activity_id)
            .map_err(|_| DatabaseTypeError)?;
        save_activity(
            db_client,
            &canonical_activity_id.to_string(),
            &self.activity,
        ).await?;
        // Immediately put into inbox if recipient is local
        for recipient in self.recipients.iter_mut() {
            // TODO: FEP-EF61: bulk add
            if recipient.is_local {
                let profile = get_remote_profile_by_actor_id(
                    db_client,
                    &recipient.id,
                ).await?;
                if profile.has_account() {
                    add_object_to_collection(
                        db_client,
                        profile.id,
                        &profile.expect_actor_data().inbox,
                        &canonical_activity_id.to_string(),
                    ).await?;
                } else {
                    log::warn!("local inbox doesn't exist: {}", recipient.inbox);
                };
                recipient.is_delivered = true;
            };
        };
        Ok(())
    }

    async fn into_job(
        self,
        db_client: &impl DatabaseClient,
        delay: u32,
    ) -> Result<(), DatabaseError> {
        if self.recipients.is_empty() {
            return Ok(());
        };
        let job_data = serde_json::to_value(self)
            .expect("activity should be serializable");
        let scheduled_for = Utc::now() + Duration::seconds(delay.into());
        enqueue_job(
            db_client,
            JobType::OutgoingActivity,
            &job_data,
            scheduled_for,
        ).await
    }

    pub async fn enqueue(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        self.into_job(db_client, 0).await
    }

    pub async fn save_and_enqueue(
        mut self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        self.save_activity(db_client).await?;
        self.enqueue(db_client).await
    }
}

const OUTGOING_QUEUE_BATCH_SIZE: u32 = 1;
const OUTGOING_QUEUE_RETRIES_MAX: u32 = 3;
const OUTGOING_QUEUE_UNREACHABLE_NORETRY: i64 = 3600 * 24 * 30; // 30 days

// 10 mins, 55 mins, 8.4 hours
pub fn outgoing_queue_backoff(failure_count: u32) -> u32 {
    debug_assert!(failure_count > 0);
    30 * (10_u32.pow(failure_count) + 10)
}

pub async fn process_queued_outgoing_activities(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), DatabaseError> {
    let db_client = get_database_client(db_pool).await?;
    let db_client_ref = &**db_client;
    let filter = FederationFilter::init(config, db_client_ref).await?;
    let batch = get_job_batch(
        db_client_ref,
        JobType::OutgoingActivity,
        OUTGOING_QUEUE_BATCH_SIZE,
        JOB_TIMEOUT,
    ).await?;
    drop(db_client);
    let instance = config.instance();
    for job in batch {
        let db_client = get_database_client(db_pool).await?;
        let db_client_ref = &**db_client;
        let mut job_data: OutgoingActivityJobData =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        let mut recipients = job_data.recipients;
        if !instance.federation.enabled {
            log::info!(
                "(private mode) not delivering activity to {} inboxes: {}",
                recipients.len(),
                job_data.activity,
            );
            delete_job_from_queue(db_client_ref, job.id).await?;
            continue;
        };
        log::info!(
            "delivering activity to {} inboxes: {}",
            recipients.len(),
            job_data.activity,
        );
        drop(db_client);

        // TODO: perform filtering in OutgoingActivityJobData::prepare_recipients
        for recipient in recipients.iter_mut() {
            if !recipient.is_finished() {
                let recipient_hostname =
                    parse_http_url_from_db(&recipient.inbox)?.hostname();
                if filter.is_action_required(
                    recipient_hostname.as_str(),
                    FilterAction::Reject,
                ) {
                    log::warn!("delivery blocked: {}", recipient.inbox);
                    recipient.is_unreachable = true;
                };
            };
        };

        let start_time = Instant::now();
        let worker_result = deliver_activity_worker(
            instance.clone(),
            job_data.sender.clone(),
            job_data.activity.clone(),
            &mut recipients,
        ).await;

        let db_client = &**get_database_client(db_pool).await?;
        match worker_result {
            Ok(_) => (),
            Err(error) => {
                // Unexpected error
                log::error!("{}", error);
                delete_job_from_queue(db_client, job.id).await?;
                continue;
            },
        };
        log::info!(
            "delivery job: {:.2?}, {} delivered, {} errors, {} skipped (attempt #{})",
            start_time.elapsed(),
            recipients.iter().filter(|item| item.is_delivered).count(),
            recipients.iter()
                .filter(|item| !item.is_delivered && !item.is_unreachable)
                .count(),
            recipients.iter()
                .filter(|item| !item.is_delivered && item.is_unreachable)
                .count(),
            job_data.failure_count + 1,
        );
        if job_data.failure_count == 0 {
            // Mark unreachable recipients after first attempt
            // TODO: O(1)
            for recipient in recipients.iter_mut() {
                if !recipient.is_delivered {
                    if recipient.is_gone {
                        // Don't retry if recipient is gone
                        recipient.is_unreachable = true;
                        continue;
                    };
                    let profile = match get_remote_profile_by_actor_id(
                        db_client,
                        &recipient.id,
                    ).await {
                        Ok(profile) => profile,
                        Err(DatabaseError::NotFound(_)) => {
                            // Recipient was deleted
                            recipient.is_unreachable = true;
                            continue;
                        },
                        Err(other_error) => return Err(other_error),
                    };
                    if let Some(unreachable_since) = profile.unreachable_since {
                        let noretry_after = unreachable_since +
                            Duration::seconds(OUTGOING_QUEUE_UNREACHABLE_NORETRY);
                        if noretry_after < Utc::now() {
                            recipient.is_unreachable = true;
                        };
                    };
                };
            };
        };
        if recipients.iter().any(|recipient| !recipient.is_finished()) &&
            job_data.failure_count < OUTGOING_QUEUE_RETRIES_MAX
        {
            job_data.failure_count += 1;
            // Re-queue if some deliveries are not successful
            job_data.recipients = recipients;
            let retry_after = outgoing_queue_backoff(job_data.failure_count);
            job_data.into_job(db_client, retry_after).await?;
            log::info!("delivery job re-queued");
        } else {
            // Update reachability statuses if all deliveries are successful
            // or if retry limit is reached
            // TODO: track reachability status of servers, not actors
            let statuses = recipients
                .into_iter()
                // Group by actor ID (could have many inboxes)
                .fold(BTreeMap::new(), |mut map: BTreeMap<_, Vec<_>>, recipient| {
                    let inboxes = map.entry(recipient.id.clone()).or_insert(vec![]);
                    inboxes.push(recipient);
                    map
                })
                .into_iter()
                .inspect(|(actor_id, inboxes)| {
                    // Log "gone" actors
                    // TODO: delete
                    if inboxes.iter().all(|inbox| inbox.is_gone) {
                        log::warn!("actor is gone: {actor_id}");
                    };
                })
                .map(|(actor_id, inboxes)| {
                    // Single successful delivery is enough
                    let is_reachable = inboxes.iter()
                        .any(|inbox| inbox.is_delivered);
                    (actor_id, !is_reachable)
                })
                .collect();
            set_reachability_status(db_client, statuses).await?;
            log::info!("reachability statuses updated");
        };
        delete_job_from_queue(db_client, job.id).await?;
    };
    Ok(())
}

#[derive(Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum FetcherJobData {
    Outbox { actor_id: String },
    Featured { actor_id: String },
    Context { object_id: String },
}

impl FetcherJobData {
    pub async fn into_job(
        self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), DatabaseError> {
        let job_data = serde_json::to_value(self)
            .expect("job data should be serializable");
        let scheduled_for = Utc::now(); // run immediately
        enqueue_job(
            db_client,
            JobType::Fetcher,
            &job_data,
            scheduled_for,
        ).await
    }
}

pub async fn fetcher_queue_executor(
    config: &Config,
    db_pool: &DatabaseConnectionPool,
) -> Result<(), DatabaseError> {
    const BATCH_SIZE: u32 = 1;
    // Re-queue running (failed) jobs after 1 hour
    const JOB_TIMEOUT: u32 = 3600;
    const COLLECTION_LIMIT: usize = 20;
    let db_client = &mut **get_database_client(db_pool).await?;
    let batch = get_job_batch(
        db_client,
        JobType::Fetcher,
        BATCH_SIZE,
        JOB_TIMEOUT,
    ).await?;
    for job in batch {
        let job_data: FetcherJobData =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        let result = match job_data {
            FetcherJobData::Outbox { actor_id } => {
                import_from_outbox(
                    config,
                    db_client,
                    &actor_id,
                    COLLECTION_LIMIT,
                ).await
            },
            FetcherJobData::Featured { actor_id } => {
                import_featured(
                    config,
                    db_client,
                    &actor_id,
                    COLLECTION_LIMIT,
                ).await
            },
            FetcherJobData::Context { object_id } => {
                import_replies(
                    config,
                    db_client,
                    &object_id,
                    false, // don't use context
                    COLLECTION_LIMIT,
                ).await
            },
        };
        result.unwrap_or_else(|error| {
            let level = if is_actor_importer_error(&error) {
                log::Level::Warn
            } else {
                log::Level::Error
            };
            log::log!(level, "background fetcher: {}", error);
        });
        delete_job_from_queue(db_client, job.id).await?;
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_outgoing_queue_backoff() {
        assert_eq!(outgoing_queue_backoff(1), 600);
        assert_eq!(outgoing_queue_backoff(2), 3300);
    }

    #[test]
    fn test_outgoing_queue_sort_recipients() {
        let instance_url = "https://local.example";
        let sender = User::default();
        let activity = json!({});
        let recipient_1 =
            Recipient::new("https://b.example/actor", "https://b.example/inbox");
        let recipient_2 =
            Recipient::new("https://a.example/actor", "https://a.example/inbox");
        let recipient_3 =
            Recipient::new("https://c.example/actor", "https://c.example/inbox");
        let recipient_4 =
            Recipient::new("https://d.example/actor", "https://d.example/inbox");
        let recipients = vec![
            recipient_3,
            recipient_1,
            recipient_2.clone(),
            {
                let mut recipient = recipient_2;
                recipient.is_primary = true;
                recipient
            },
            {
                let mut recipient = recipient_4;
                recipient.is_primary = true;
                recipient
            },
        ];
        let job_data = OutgoingActivityJobData::new(
            instance_url,
            &sender,
            activity,
            recipients,
        );
        assert_eq!(job_data.recipients.len(), 4);
        assert_eq!(job_data.recipients[0].id, "https://a.example/actor");
        assert_eq!(job_data.recipients[0].is_primary, true);
        assert_eq!(job_data.recipients[1].id, "https://d.example/actor");
        assert_eq!(job_data.recipients[1].is_primary, true);
        assert_eq!(job_data.recipients[2].id, "https://b.example/actor");
        assert_eq!(job_data.recipients[2].is_primary, false);
        assert_eq!(job_data.recipients[3].id, "https://c.example/actor");
        assert_eq!(job_data.recipients[3].is_primary, false);
    }
}
