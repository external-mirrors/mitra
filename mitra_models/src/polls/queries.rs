use std::collections::HashSet;

use uuid::Uuid;

use mitra_utils::id::generate_ulid;

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
        DatabaseTypeError,
    },
    profiles::types::DbActorProfile,
};

use super::types::{Poll, PollData, PollResult, PollResults, PollVote};

pub async fn create_poll(
    db_client: &impl DatabaseClient,
    post_id: Uuid,
    poll_data: PollData,
) -> Result<Poll, DatabaseError> {
    let row = db_client.query_one(
        "
        INSERT INTO poll (
            id,
            multiple_choices,
            ends_at,
            results
        )
        VALUES ($1, $2, $3, $4)
        RETURNING poll
        ",
        &[
            &post_id,
            &poll_data.multiple_choices,
            &poll_data.ends_at,
            &PollResults::new(poll_data.results),
        ],
    ).await?;
    let poll = row.try_get("poll")?;
    Ok(poll)
}

pub async fn update_poll(
    db_client: &impl DatabaseClient,
    poll_id: Uuid,
    poll_data: PollData,
) -> Result<(Poll, bool), DatabaseError> {
    let maybe_row = db_client.query_opt(
        "
        UPDATE poll
        SET
            multiple_choices = $2,
            ends_at = $3,
            results = $4
        WHERE id = $1
        RETURNING
            poll,
            (SELECT poll FROM poll WHERE id = $1) AS poll_old
        ",
        &[
            &poll_id,
            &poll_data.multiple_choices,
            &poll_data.ends_at,
            &PollResults::new(poll_data.results),
        ],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("poll"))?;
    let poll: Poll = row.try_get("poll")?;
    let poll_old: Poll = row.try_get("poll_old")?;
    let get_options = |results: &PollResults| -> HashSet<String> {
        results.inner().iter()
            .map(|result| result.option_name.clone())
            .collect()
    };
    let options_changed =
        get_options(&poll.results) != get_options(&poll_old.results) ||
        poll.multiple_choices != poll_old.multiple_choices;
    Ok((poll, options_changed))
}

async fn create_votes(
    db_client: &impl DatabaseClient,
    poll_id: Uuid,
    voter_id: Uuid,
    choices: &[String],
) -> Result<Vec<PollVote>, DatabaseError> {
    if choices.is_empty() {
        return Err(DatabaseTypeError.into());
    };
    let ids: Vec<_> = choices.iter().map(|_| generate_ulid()).collect();
    let rows = db_client.query(
        "
        INSERT INTO poll_vote (
            id,
            poll_id,
            voter_id,
            choice
        )
        SELECT unnest($1::uuid[]), $2, $3, unnest($4::text[])
        WHERE NOT EXISTS (
            SELECT 1 FROM poll_vote
            WHERE poll_id = $2 AND voter_id = $3
        )
        RETURNING poll_vote
        ",
        &[
            &ids,
            &poll_id,
            &voter_id,
            &choices,
        ],
    ).await?;
    if rows.len() == 0 {
        return Err(DatabaseError::AlreadyExists("poll vote"));
    };
    let votes: Vec<PollVote> = rows.iter()
        .map(|row| row.try_get("poll_vote"))
        .collect::<Result<_, _>>()?;
    assert_eq!(votes.len(), choices.len());
    Ok(votes)
}

pub async fn reset_votes(
    db_client: &impl DatabaseClient,
    poll_id: Uuid,
) -> Result<(), DatabaseError> {
    db_client.execute(
        "
        DELETE FROM poll_vote
        WHERE poll_id = $1
        ",
        &[&poll_id],
    ).await?;
    Ok(())
}

#[allow(dead_code)]
async fn get_poll_results(
    db_client: &impl DatabaseClient,
    poll_id: Uuid,
) -> Result<Vec<PollResult>, DatabaseError> {
    // Only accurate if poll is local
    let rows = db_client.query(
        "
        SELECT poll_option.name, count(voter_id)
        FROM poll
        CROSS JOIN LATERAL
            ROWS FROM (jsonb_to_recordset(poll.results) AS (option_name TEXT))
            WITH ORDINALITY AS poll_option (name, ordinality)
        LEFT JOIN poll_vote ON (
            poll_vote.poll_id = poll.id
            AND poll_vote.choice = poll_option.name
        )
        WHERE poll.id = $1
        GROUP BY poll_option.name, poll_option.ordinality
        ORDER BY poll_option.ordinality
        ",
        &[&poll_id],
    ).await?;
    let mut results = vec![];
    for row in rows {
        let name = row.try_get("name")?;
        let count: i64 = row.try_get("count")?;
        let count = count.try_into().map_err(|_| DatabaseTypeError)?;
        let result = PollResult {
            option_name: name,
            vote_count: count,
        };
        results.push(result);
    };
    Ok(results)
}

// Creates a single vote (needed for remote votes)
pub async fn vote_one(
    db_client: &mut impl DatabaseClient,
    poll_id: Uuid,
    voter_id: Uuid,
    choice: &str,
    object_id: &str,
) -> Result<Poll, DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        SELECT poll.results
        FROM poll WHERE poll.id = $1
        FOR UPDATE
        ",
        &[&poll_id],
    ).await?;

    let row = maybe_row.ok_or(DatabaseError::NotFound("poll"))?;
    let results: PollResults = row.try_get("results")?;
    let mut results = results.into_inner();
    let result: &mut _ = results.iter_mut()
        .find(|result| result.option_name == choice)
        .ok_or(DatabaseError::NotFound("poll option"))?;
    result.vote_count += 1;
    // Create vote
    let vote_id = generate_ulid();
    let inserted_count = transaction.execute(
        "
        INSERT INTO poll_vote (
            id,
            poll_id,
            voter_id,
            choice,
            object_id
        )
        SELECT $1, $2, $3, $4, $5
        WHERE NOT EXISTS (
            SELECT 1 FROM poll_vote
            JOIN poll ON poll.id = poll_vote.poll_id
            WHERE poll_id = $2
                AND voter_id = $3
                AND poll.multiple_choices IS FALSE
        )
        ON CONFLICT DO NOTHING
        ",
        &[
            &vote_id,
            &poll_id,
            &voter_id,
            &choice,
            &object_id,
        ],
    ).await?;
    if inserted_count == 0 {
        // Return AlreadyExists if this is not a first vote
        // in a single-choice poll or if a vote for the given option
        // already exists.
        return Err(DatabaseError::AlreadyExists("poll vote"));
    };
    // Update poll results
    let row = transaction.query_one(
        "
        UPDATE poll
        SET results = $2
        WHERE poll.id = $1
        RETURNING poll
        ",
        &[&poll_id, &PollResults::new(results)],
    ).await?;
    let poll = row.try_get("poll")?;
    transaction.commit().await?;
    Ok(poll)
}

// Creates multiple votes, and finalizes them
pub async fn vote(
    db_client: &mut impl DatabaseClient,
    poll_id: Uuid,
    voter_id: Uuid,
    choices_indices: HashSet<usize>,
) -> Result<(Poll, Vec<PollVote>), DatabaseError> {
    let transaction = db_client.transaction().await?;
    let maybe_row = transaction.query_opt(
        "
        SELECT poll.results
        FROM poll WHERE poll.id = $1
        FOR UPDATE
        ",
        &[&poll_id],
    ).await?;
    let row = maybe_row.ok_or(DatabaseError::NotFound("poll"))?;
    let results: PollResults = row.try_get("results")?;
    let mut results = results.into_inner();
    let mut choices = vec![];
    for choice_index in choices_indices {
        let maybe_result = results.get_mut(choice_index);
        let result: &mut _ = maybe_result.ok_or(DatabaseError::NotFound("poll option"))?;
        choices.push(result.option_name.clone());
        result.vote_count += 1;
    };
    // Raises an AlreadyExists error if votes are already recorded
    let votes = create_votes(&transaction, poll_id, voter_id, &choices).await?;
    // Update poll results
    let row = transaction.query_one(
        "
        UPDATE poll
        SET results = $2
        WHERE poll.id = $1
        RETURNING poll
        ",
        &[&poll_id, &PollResults::new(results)],
    ).await?;
    let poll = row.try_get("poll")?;
    transaction.commit().await?;
    Ok((poll, votes))
}

pub async fn get_voters(
    db_client: &impl DatabaseClient,
    poll_id: Uuid,
) -> Result<Vec<DbActorProfile>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT DISTINCT actor_profile
        FROM poll_vote
        JOIN actor_profile ON poll_vote.voter_id = actor_profile.id
        WHERE poll_vote.poll_id = $1
        ",
        &[&poll_id],
    ).await?;
    let profiles = rows.iter()
        .map(DbActorProfile::try_from)
        .collect::<Result<_, _>>()?;
    Ok(profiles)
}

pub(crate) async fn find_votes_by_user(
    db_client: &impl DatabaseClient,
    user_id: Uuid,
    post_ids: &[Uuid],
) -> Result<Vec<(Uuid, Vec<String>)>, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT
            poll.id,
            array_agg(poll_vote.choice) AS choices
        FROM poll_vote
        JOIN poll ON poll.id = poll_vote.poll_id
        WHERE voter_id = $1 AND poll_id = ANY($2)
        GROUP BY poll.id
        ",
        &[&user_id, &post_ids],
    ).await?;
    let votes = rows.iter()
        .map(|row| {
            let post_id = row.try_get("id")?;
            let choices = row.try_get("choices")?;
            Ok((post_id, choices))
        })
        .collect::<Result<Vec<_>, DatabaseError>>()?;
    Ok(votes)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};
    use serial_test::serial;
    use crate::{
        database::test_utils::create_test_database,
        polls::{
            test_utils::create_test_local_poll,
            types::PollResult,
        },
        posts::test_utils::create_test_local_post,
        profiles::test_utils::create_test_remote_profile,
        users::test_utils::create_test_user,
    };
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_create_poll() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "test").await;
        let post = create_test_local_post(db_client, author.id, "test").await;
        let poll_results = vec![
            PollResult { option_name: "1".to_string(), vote_count: 0 },
            PollResult { option_name: "2".to_string(), vote_count: 0 },
        ];
        let poll_ends_at = Utc.with_ymd_and_hms(2025, 5, 15, 0, 0, 0).unwrap();
        let poll_data = PollData {
            multiple_choices: false,
            ends_at: poll_ends_at,
            results: poll_results.clone(),
        };
        let poll = create_poll(
            db_client,
            post.id,
            poll_data,
        ).await.unwrap();

        assert_eq!(poll.id, post.id);
        assert_eq!(poll.multiple_choices, false);
        assert_eq!(poll.ends_at, poll_ends_at);
        assert_eq!(poll.results.inner(), poll_results);
    }

    #[tokio::test]
    #[serial]
    async fn test_update_poll() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let post = create_test_local_poll(
            db_client,
            author.id,
            &["1", "2"],
            false,
        ).await;
        let poll = post.poll.unwrap();

        // Update tallies
        let mut results_updated = poll.results.into_inner();
        results_updated[0].vote_count = 10;
        results_updated[1].vote_count = 5;
        let poll_data = PollData {
            multiple_choices: poll.multiple_choices,
            ends_at: poll.ends_at,
            results: results_updated.clone(),
        };
        let (poll, options_changed) = update_poll(
            db_client,
            post.id,
            poll_data,
        ).await.unwrap();
        assert_eq!(poll.id, post.id);
        assert_eq!(poll.results.inner(), results_updated);
        assert_eq!(options_changed, false);

        // Add new option
        let mut results_updated = poll.results.into_inner();
        results_updated.push(PollResult {
            option_name: "3".to_string(),
            vote_count: 5,
        });
        let poll_data = PollData {
            multiple_choices: poll.multiple_choices,
            ends_at: poll.ends_at,
            results: results_updated.clone(),
        };
        let (poll, options_changed) = update_poll(
            db_client,
            post.id,
            poll_data,
        ).await.unwrap();
        assert_eq!(poll.results.inner(), results_updated);
        assert_eq!(options_changed, true);
    }

    #[tokio::test]
    #[serial]
    async fn test_create_votes() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let voter_1 = create_test_user(db_client, "voter_1").await;
        let voter_2 = create_test_user(db_client, "voter_2").await;
        let option_1 = "1";
        let option_2 = "2";
        let post = create_test_local_poll(
            db_client,
            author.id,
            &[option_1, option_2],
            false,
        ).await;

        let voter_1_choices = vec![option_1.to_string()];
        let votes = create_votes(
            db_client,
            post.id,
            voter_1.id,
            &voter_1_choices,
        ).await.unwrap();
        assert_eq!(votes.len(), 1);
        let results = get_poll_results(db_client, post.id).await.unwrap();
        assert_eq!(results[0].vote_count, 1);
        assert_eq!(results[1].vote_count, 0);
        assert_eq!(results.len(), 2);
        let error = create_votes(
            db_client,
            post.id,
            voter_1.id,
            &voter_1_choices,
        ).await.err().unwrap();
        assert!(matches!(error, DatabaseError::AlreadyExists("poll vote")));

        let voter_2_choices = vec![option_2.to_string()];
        let votes = create_votes(
            db_client,
            post.id,
            voter_2.id,
            &voter_2_choices,
        ).await.unwrap();
        assert_eq!(votes.len(), 1);
        let results = get_poll_results(db_client, post.id).await.unwrap();
        assert_eq!(results[0].vote_count, 1);
        assert_eq!(results[1].vote_count, 1);
        assert_eq!(results.len(), 2);
    }

    #[tokio::test]
    #[serial]
    async fn test_vote_one_single_choice() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let voter = create_test_remote_profile(
            db_client,
            "voter",
            "remote.example",
            "https://remote.example/actor",
        ).await;
        let option_1 = "1";
        let option_2 = "2";
        let post = create_test_local_poll(
            db_client,
            author.id,
            &[option_1, option_2],
            false,
        ).await;

        let poll_updated = vote_one(
            db_client,
            post.id,
            voter.id,
            option_1,
            "https://remote.example/votes/1",
        ).await.unwrap();
        let results = poll_updated.results.into_inner();
        assert_eq!(results[0].vote_count, 1);
        assert_eq!(results[1].vote_count, 0);

        let error = vote_one(
            db_client,
            post.id,
            voter.id,
            option_2,
            "https://remote.example/votes/2",
        ).await.err().unwrap();
        assert!(matches!(error, DatabaseError::AlreadyExists("poll vote")));
    }

    #[tokio::test]
    #[serial]
    async fn test_vote_one_multiple_choices() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let voter = create_test_remote_profile(
            db_client,
            "voter",
            "remote.example",
            "https://remote.example/actor",
        ).await;
        let option_1 = "1";
        let option_2 = "2";
        let post = create_test_local_poll(
            db_client,
            author.id,
            &[option_1, option_2],
            true,
        ).await;

        let poll_updated = vote_one(
            db_client,
            post.id,
            voter.id,
            option_1,
            "https://remote.example/votes/1",
        ).await.unwrap();
        let results = poll_updated.results.into_inner();
        assert_eq!(results[0].vote_count, 1);
        assert_eq!(results[1].vote_count, 0);

        let poll_updated = vote_one(
            db_client,
            post.id,
            voter.id,
            option_2,
            "https://remote.example/votes/2",
        ).await.unwrap();
        let results = poll_updated.results.into_inner();
        assert_eq!(results[0].vote_count, 1);
        assert_eq!(results[1].vote_count, 1);

        let error = vote_one(
            db_client,
            post.id,
            voter.id,
            option_2,
            "https://remote.example/votes/3",
        ).await.err().unwrap();
        assert!(matches!(error, DatabaseError::AlreadyExists("poll vote")));
    }

    #[tokio::test]
    #[serial]
    async fn test_vote() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let voter = create_test_user(db_client, "voter").await;
        let option_1 = "1";
        let option_2 = "2";
        let post = create_test_local_poll(
            db_client,
            author.id,
            &[option_1, option_2],
            false,
        ).await;

        let (poll, votes) = vote(
            db_client,
            post.id,
            voter.id,
            HashSet::from([1]),
        ).await.unwrap();
        let results = poll.results.into_inner();
        assert_eq!(results[0].vote_count, 0);
        assert_eq!(results[1].vote_count, 1);
        assert_eq!(votes[0].voter_id, voter.id);
        assert_eq!(votes[0].choice, option_2);
        assert_eq!(votes.len(), 1);

        let error = vote(
            db_client,
            post.id,
            voter.id,
            HashSet::from([0]),
        ).await.err().unwrap();
        assert!(matches!(error, DatabaseError::AlreadyExists("poll vote")));
    }

    #[tokio::test]
    #[serial]
    async fn test_get_voters() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let voter_1 = create_test_user(db_client, "voter_1").await;
        let voter_2 = create_test_user(db_client, "voter_2").await;
        let option_1 = "1";
        let option_2 = "2";
        let post = create_test_local_poll(
            db_client,
            author.id,
            &[option_1, option_2],
            false,
        ).await;
        vote(
            db_client,
            post.id,
            voter_1.id,
            HashSet::from([0]),
        ).await.unwrap();
        vote(
            db_client,
            post.id,
            voter_2.id,
            HashSet::from([1]),
        ).await.unwrap();
        let voters = get_voters(db_client, post.id).await.unwrap();
        assert_eq!(voters.len(), 2);
    }

    #[tokio::test]
    #[serial]
    async fn test_find_votes_by_user() {
        let db_client = &mut create_test_database().await;
        let author = create_test_user(db_client, "author").await;
        let voter = create_test_user(db_client, "voter").await;
        let option_1 = "1";
        let option_2 = "2";
        let post = create_test_local_poll(
            db_client,
            author.id,
            &[option_1, option_2],
            false,
        ).await;
        vote(
            db_client,
            post.id,
            voter.id,
            HashSet::from([1]),
        ).await.unwrap();
        let votes = find_votes_by_user(
            db_client,
            voter.id,
            &[post.id],
        ).await.unwrap();
        assert_eq!(votes.len(), 1);
        let vote = &votes[0];
        assert_eq!(vote.0, post.id);
        assert_eq!(vote.1, vec![option_2.to_string()]);
    }
}
