use std::collections::HashSet;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use mitra_models::polls::types::{Poll as DbPoll};

use crate::mastodon_api::custom_emojis::types::CustomEmoji;

#[derive(Serialize)]
struct PollOption {
    title: String,
    votes_count: u32,
}

// https://docs.joinmastodon.org/entities/Poll
#[derive(Serialize)]
pub struct Poll {
    id: Uuid,
    expires_at: DateTime<Utc>,
    expired: bool,
    multiple: bool,
    votes_count: u32,
    voters_count: Option<u32>,
    options: Vec<PollOption>,
    emojis: Vec<CustomEmoji>,

    voted: Option<bool>,
    own_votes: Option<Vec<usize>>,
}

impl Poll {
    pub fn from_db(db_poll: &DbPoll, maybe_voted_for: Option<Vec<String>>) -> Self {
        let mut options = vec![];
        let mut votes_count = 0;
        for result in db_poll.results.inner() {
            let option = PollOption {
                title: result.option_name.clone(),
                votes_count: result.vote_count,
            };
            options.push(option);
            votes_count += result.vote_count;
        };
        let maybe_own_votes = if let Some(ref voted_for) = maybe_voted_for {
            let mut own_votes = vec![];
            for (index, result) in db_poll.results.inner().iter().enumerate() {
                if voted_for.contains(&result.option_name) {
                    own_votes.push(index);
                };
            };
            Some(own_votes)
        } else {
            None
        };
        Self {
            id: db_poll.id,
            expires_at: db_poll.ends_at,
            expired: db_poll.ended(),
            multiple: db_poll.multiple_choices,
            votes_count: votes_count,
            voters_count: db_poll.multiple_choices.then_some(0),
            options: options,
            emojis: vec![],
            voted: maybe_own_votes.as_ref().map(|own_votes| !own_votes.is_empty()),
            own_votes: maybe_own_votes,
        }
    }
}

#[derive(Deserialize)]
pub struct VoteData {
    #[serde(alias = "choices[]")]
    pub choices: HashSet<usize>,
}
