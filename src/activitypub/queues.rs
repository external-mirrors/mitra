use std::time::Instant;

use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use uuid::Uuid;

use mitra_activitypub::fetch::FetchError;
use mitra_config::Config;
use mitra_models::{
    background_jobs::queries::{
        enqueue_job,
        get_job_batch,
        delete_job_from_queue,
    },
    background_jobs::types::JobType,
    database::{
        get_database_client,
        DatabaseClient,
        DatabaseError,
        DatabaseTypeError,
        DbPool,
    },
    profiles::queries::{
        get_profile_by_remote_actor_id,
        set_reachability_status,
    },
    users::queries::get_user_by_id,
};

use super::deliverer::{deliver_activity_worker, Recipient};
use super::receiver::{handle_activity, HandlerError};

const JOB_TIMEOUT: u32 = 3600; // 1 hour

#[derive(Deserialize, Serialize)]
pub struct IncomingActivityJobData {
    activity: Value,
    is_authenticated: bool,
    failure_count: u32,
}

impl IncomingActivityJobData {
    pub fn new(activity: &Value, is_authenticated: bool) -> Self {
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
            &JobType::IncomingActivity,
            &job_data,
            &scheduled_for,
        ).await
    }
}

const INCOMING_QUEUE_BATCH_SIZE: u32 = 10;
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
        &JobType::IncomingActivity,
        INCOMING_QUEUE_BATCH_SIZE,
        JOB_TIMEOUT,
    ).await?;
    for job in batch {
        let mut job_data: IncomingActivityJobData =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        // See also: activitypub::queues::JOB_TIMEOUT
        let duration_max = std::time::Duration::from_secs(600);
        let handler_future = handle_activity(
            config,
            db_client,
            &job_data.activity,
            job_data.is_authenticated,
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
                delete_job_from_queue(db_client, &job.id).await?;
                continue;
            },
        };
        if let Err(error) = handler_result {
            job_data.failure_count += 1;
            if let HandlerError::DatabaseError(
                DatabaseError::DatabaseClientError(ref pg_error)) = error
            {
                log::error!("database client error: {}", pg_error);
            };
            log::warn!(
                "failed to process activity ({}) (attempt #{}): {}",
                error,
                job_data.failure_count,
                job_data.activity,
            );
            if job_data.failure_count <= INCOMING_QUEUE_RETRIES_MAX &&
                // Don't retry after fetcher recursion error
                !matches!(error, HandlerError::FetchError(
                    FetchError::RecursionError |
                    FetchError::NotFound(_)
                ))
            {
                // Re-queue
                let retry_after = incoming_queue_backoff(job_data.failure_count);
                job_data.into_job(db_client, retry_after).await?;
                log::info!("activity re-queued");
            };
        };
        delete_job_from_queue(db_client, &job.id).await?;
    };
    Ok(())
}

#[derive(Deserialize, Serialize)]
pub struct OutgoingActivityJobData {
    pub activity: Value,
    pub sender_id: Uuid,
    pub recipients: Vec<Recipient>,
    pub failure_count: u32,
}

impl OutgoingActivityJobData {
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
            &JobType::OutgoingActivity,
            &job_data,
            &scheduled_for,
        ).await
    }
}

const OUTGOING_QUEUE_BATCH_SIZE: u32 = 1;
const OUTGOING_QUEUE_RETRIES_MAX: u32 = 3;
const OUTGOING_QUEUE_UNREACHABLE_NORETRY: i64 = 3600 * 24 * 30; // 30 days

// 5 mins, 50 mins, 8 hours
pub fn outgoing_queue_backoff(failure_count: u32) -> u32 {
    debug_assert!(failure_count > 0);
    30 * 10_u32.pow(failure_count)
}

pub async fn process_queued_outgoing_activities(
    config: &Config,
    db_pool: &DbPool,
) -> Result<(), DatabaseError> {
    let db_client = &**get_database_client(db_pool).await?;
    let batch = get_job_batch(
        db_client,
        &JobType::OutgoingActivity,
        OUTGOING_QUEUE_BATCH_SIZE,
        JOB_TIMEOUT,
    ).await?;
    for job in batch {
        let mut job_data: OutgoingActivityJobData =
            serde_json::from_value(job.job_data)
                .map_err(|_| DatabaseTypeError)?;
        let sender = get_user_by_id(db_client, &job_data.sender_id).await?;
        let mut recipients = job_data.recipients;
        let start_time = Instant::now();
        match deliver_activity_worker(
            config.instance(),
            sender,
            job_data.activity.clone(),
            &mut recipients,
        ).await {
            Ok(_) => (),
            Err(error) => {
                // Unexpected error
                log::error!("{}", error);
                delete_job_from_queue(db_client, &job.id).await?;
                return Ok(());
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
                    let profile = get_profile_by_remote_actor_id(
                        db_client,
                        &recipient.id,
                    ).await?;
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
            // Update inbox status if all deliveries are successful
            // or if retry limit is reached
            for recipient in recipients {
                // TODO: O(1)
                set_reachability_status(
                    db_client,
                    &recipient.id,
                    recipient.is_delivered,
                ).await?;
            };
        };
        delete_job_from_queue(db_client, &job.id).await?;
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_outgoing_queue_backoff() {
        assert_eq!(outgoing_queue_backoff(1), 300);
        assert_eq!(outgoing_queue_backoff(2), 3000);
    }
}
