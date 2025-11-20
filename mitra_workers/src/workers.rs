use std::collections::HashMap;
use std::time::Duration;

use chrono::{DateTime, Utc};

use mitra_config::Config;
use mitra_models::database::DatabaseConnectionPool;

use super::periodic_tasks::*;

const WORKER_DELAY: u64 = 500;

#[derive(Debug, Eq, Hash, PartialEq)]
pub enum PeriodicTask {
    IncomingActivityQueueExecutor,
    OutgoingActivityQueueExecutor,
    FetcherQueueExecutor,
    DeleteExtraneousPosts,
    DeleteEmptyProfiles,
    PruneTags,
    PruneRemoteEmojis,
    PruneUnusedAttachments,
    PruneActivityPubObjects,
    PruneActivityPubCollectionItems,
    MediaCleanupQueueExecutor,
    RefreshMaterializedViews,
    ImporterQueueExecutor,
    RemoteInvoiceMonitor,
    SubscriptionExpirationMonitor,
    MoneroPaymentMonitor,
    MoneroRecurrentPaymentMonitor,
}

impl PeriodicTask {
    /// Returns task period (in seconds)
    fn period(&self) -> i64 {
        match self {
            Self::IncomingActivityQueueExecutor => 1,
            Self::OutgoingActivityQueueExecutor => 1,
            Self::FetcherQueueExecutor => 10,
            Self::DeleteExtraneousPosts => 3600,
            Self::DeleteEmptyProfiles => 3600,
            Self::PruneTags => 3600,
            Self::PruneRemoteEmojis => 3600,
            Self::PruneUnusedAttachments => 3600,
            Self::PruneActivityPubObjects => 3600,
            Self::PruneActivityPubCollectionItems => 3600,
            Self::MediaCleanupQueueExecutor => 10,
            Self::RefreshMaterializedViews => 600,
            Self::ImporterQueueExecutor => 60,
            Self::RemoteInvoiceMonitor => 600,
            Self::SubscriptionExpirationMonitor => 300,
            Self::MoneroPaymentMonitor => 30,
            Self::MoneroRecurrentPaymentMonitor => 600,
        }
    }

    fn is_ready(&self, last_run: &Option<DateTime<Utc>>) -> bool {
        match last_run {
            Some(last_run) => {
                let time_passed = Utc::now() - *last_run;
                time_passed.num_seconds() >= self.period()
            },
            None => true,
        }
    }
}

pub async fn run_worker(
    config: Config,
    db_pool: DatabaseConnectionPool,
    tasks: Vec<PeriodicTask>,
) -> () {
    let mut worker_state: HashMap<PeriodicTask, Option<DateTime<Utc>>> =
        HashMap::from_iter(tasks.into_iter().map(|task| (task, None)));
    let mut interval =
        tokio::time::interval(Duration::from_millis(WORKER_DELAY));
    loop {
        interval.tick().await;

        for (task, last_run) in worker_state.iter_mut() {
            if !task.is_ready(last_run) {
                continue;
            };
            let task_result = match task {
                PeriodicTask::IncomingActivityQueueExecutor => {
                    incoming_activity_queue_executor(&config, &db_pool).await
                },
                PeriodicTask::OutgoingActivityQueueExecutor => {
                    outgoing_activity_queue_executor(&config, &db_pool).await
                },
                PeriodicTask::FetcherQueueExecutor => {
                    fetcher_queue_executor(&config, &db_pool).await
                        .map_err(Into::into)
                },
                PeriodicTask::DeleteExtraneousPosts => {
                    delete_extraneous_posts(&config, &db_pool).await
                },
                PeriodicTask::DeleteEmptyProfiles => {
                    delete_empty_profiles(&config, &db_pool).await
                },
                PeriodicTask::PruneTags => {
                    prune_tags(&config, &db_pool).await
                },
                PeriodicTask::PruneRemoteEmojis => {
                    prune_remote_emojis(&config, &db_pool).await
                },
                PeriodicTask::PruneUnusedAttachments => {
                    prune_unused_attachments(&config, &db_pool).await
                },
                PeriodicTask::PruneActivityPubObjects => {
                    prune_activitypub_objects(&config, &db_pool).await
                },
                PeriodicTask::PruneActivityPubCollectionItems => {
                    prune_activitypub_collection_items(&config, &db_pool).await
                },
                PeriodicTask::MediaCleanupQueueExecutor => {
                    media_cleanup_queue_executor(&config, &db_pool).await
                },
                PeriodicTask::RefreshMaterializedViews => {
                    refresh_materialized_views(&db_pool).await
                },
                PeriodicTask::ImporterQueueExecutor => {
                    importer_queue_executor(&config, &db_pool).await
                },
                PeriodicTask::RemoteInvoiceMonitor => {
                    remote_invoice_monitor(&config, &db_pool).await
                },
                PeriodicTask::SubscriptionExpirationMonitor => {
                    subscription_expiration_monitor(&config, &db_pool).await
                },
                PeriodicTask::MoneroPaymentMonitor => {
                    monero_payment_monitor(&config, &db_pool).await
                },
                PeriodicTask::MoneroRecurrentPaymentMonitor => {
                    monero_recurrent_payment_monitor(&config, &db_pool).await
                },
            };
            task_result.unwrap_or_else(|err| {
                log::error!("{:?}: {}", task, err);
            });
            *last_run = Some(Utc::now());
        };
    };
}

fn start_main_worker(
    config: Config,
    db_pool: DatabaseConnectionPool,
) -> () {
    tokio::spawn(async move {
        let mut tasks = vec![
            PeriodicTask::FetcherQueueExecutor,
            PeriodicTask::PruneTags,
            PeriodicTask::PruneRemoteEmojis,
            PeriodicTask::PruneUnusedAttachments,
            PeriodicTask::PruneActivityPubObjects,
            PeriodicTask::PruneActivityPubCollectionItems,
            PeriodicTask::MediaCleanupQueueExecutor,
            PeriodicTask::RefreshMaterializedViews,
            PeriodicTask::ImporterQueueExecutor,
            PeriodicTask::RemoteInvoiceMonitor,
            PeriodicTask::SubscriptionExpirationMonitor,
        ];
        if !config.federation.incoming_queue_worker_enabled {
            tasks.push(PeriodicTask::IncomingActivityQueueExecutor);
        };
        if !config.federation.deliverer_standalone {
            tasks.push(PeriodicTask::OutgoingActivityQueueExecutor);
        };
        if config.retention.extraneous_posts.is_some() {
            tasks.push(PeriodicTask::DeleteExtraneousPosts);
        };
        if config.retention.empty_profiles.is_some() {
            tasks.push(PeriodicTask::DeleteEmptyProfiles);
        };
        if config.monero_config().is_some() {
            tasks.push(PeriodicTask::MoneroPaymentMonitor);
            tasks.push(PeriodicTask::MoneroRecurrentPaymentMonitor);
        };
        run_worker(config, db_pool, tasks).await;
    });
}

fn start_outgoing_activity_queue_worker(
    config: Config,
    db_pool: DatabaseConnectionPool,
) -> () {
    assert!(config.federation.deliverer_standalone);
    tokio::spawn(async move {
        let tasks = vec![PeriodicTask::OutgoingActivityQueueExecutor];
        run_worker(config, db_pool, tasks).await;
    });
}

fn start_incoming_activity_queue_worker(
    config: Config,
    db_pool: DatabaseConnectionPool,
) -> () {
    assert!(config.federation.incoming_queue_worker_enabled);
    tokio::spawn(async move {
        let tasks = vec![PeriodicTask::IncomingActivityQueueExecutor];
        run_worker(config, db_pool, tasks).await;
    });
}

pub fn start_workers(
    config: Config,
    db_pool: DatabaseConnectionPool,
) -> () {
    start_main_worker(
        config.clone(),
        db_pool.clone(),
    );
    log::info!("background worker started");

    if config.federation.incoming_queue_worker_enabled {
        start_incoming_activity_queue_worker(
            config.clone(),
            db_pool.clone(),
        );
        log::info!("incoming activity queue worker started");
    };

    if config.federation.deliverer_standalone {
        start_outgoing_activity_queue_worker(
            config.clone(),
            db_pool.clone(),
        );
        log::info!("outgoing activity queue worker started");
    };
}
