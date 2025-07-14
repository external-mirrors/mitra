use anyhow::Error;
use clap::Parser;

use mitra_config::Config;
use mitra_models::{
    database::DatabaseConnectionPool,
};
use mitra_workers::workers::{run_worker, PeriodicTask};

#[derive(Parser)]
#[clap(hide = true)]
pub struct Worker {
    task: String,
}

impl Worker {
    pub async fn execute(
        &self,
        config: Config,
        db_pool: DatabaseConnectionPool,
    ) -> Result<(), Error> {
        let task = match self.task.as_str() {
            "incoming-queue" => PeriodicTask::IncomingActivityQueueExecutor,
            "outgoing-queue" => PeriodicTask::OutgoingActivityQueueExecutor,
            _ => return Err(Error::msg("unexpected task name")),
        };
        run_worker(config, db_pool, vec![task]).await;
        Ok(())
    }
}
