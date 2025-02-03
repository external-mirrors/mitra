use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::database::json_macro::{json_from_sql, json_to_sql};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct PollResult {
    pub option_name: String,
    pub vote_count: u32,
}

impl PollResult {
    pub fn new(option_name: &str) -> Self {
        Self { option_name: option_name.to_string(), vote_count: 0 }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PollResults(Vec<PollResult>);

impl PollResults {
    pub fn new(results: Vec<PollResult>) -> Self {
        Self(results)
    }

    pub fn inner(&self) -> &[PollResult] {
        &self.0
    }

    pub fn into_inner(self) -> Vec<PollResult> {
        self.0
    }
}

json_from_sql!(PollResults);
json_to_sql!(PollResults);

#[derive(Clone, FromSql)]
#[postgres(name = "poll")]
pub struct Poll {
    pub id: Uuid,
    pub multiple_choices: bool,
    pub ends_at: DateTime<Utc>,
    pub results: PollResults,
}

impl Poll {
    pub fn ended(&self) -> bool {
        self.ends_at < Utc::now()
    }
}

pub struct PollData {
    pub multiple_choices: bool,
    pub ends_at: DateTime<Utc>,
    pub results: Vec<PollResult>,
}

impl From<Poll> for PollData {
    fn from(poll: Poll) -> Self {
        Self {
            multiple_choices: poll.multiple_choices,
            ends_at: poll.ends_at,
            results: poll.results.inner().to_vec(),
        }
    }
}

#[derive(Clone, FromSql)]
#[postgres(name = "poll_vote")]
pub struct PollVote {
    pub id: Uuid,
    pub poll_id: Uuid,
    pub voter_id: Uuid,
    pub choice: String,
    pub object_id: Option<String>,
}
