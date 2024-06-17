use uuid::Uuid;

use mitra_federation::addresses::ActorAddress;
use mitra_models::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
    profiles::types::DbActorProfile,
    relationships::queries::{get_followers, get_following},
};
use mitra_validators::errors::ValidationError;

const IMPORTER_JOB_LIMIT: usize = 500;

fn export_profiles_to_csv(
    local_hostname: &str,
    profiles: Vec<DbActorProfile>,
) -> String {
    let mut csv = String::new();
    for profile in profiles {
        let actor_address = ActorAddress::new_unchecked(
            &profile.username,
            profile.hostname.as_deref().unwrap_or(local_hostname),
        );
        csv += &format!("{}\n", actor_address);
    };
    csv
}

pub async fn export_followers(
    db_client: &impl DatabaseClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let followers = get_followers(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, followers);
    Ok(csv)
}

pub async fn export_follows(
    db_client: &impl DatabaseClient,
    local_hostname: &str,
    user_id: &Uuid,
) -> Result<String, DatabaseError> {
    let following = get_following(db_client, user_id).await?;
    let csv = export_profiles_to_csv(local_hostname, following);
    Ok(csv)
}

pub fn parse_address_list(csv: &str)
    -> Result<Vec<ActorAddress>, ValidationError>
{
    let mut addresses: Vec<_> = csv.lines()
        .filter_map(|line| line.split(',').next())
        .map(|line| line.trim().to_string())
        // Skip header and empty lines
        .filter(|line| line != "Account address" && !line.is_empty())
        .map(|line| ActorAddress::from_handle(&line))
        .collect::<Result<_, _>>()
        .map_err(|error| ValidationError(error.message()))?;
    addresses.sort();
    addresses.dedup();
    if addresses.len() > IMPORTER_JOB_LIMIT {
        return Err(ValidationError("can't process more than 500 items at once"));
    };
    Ok(addresses)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_export_profiles_to_csv() {
        let profile_1 = DbActorProfile::local_for_test("user1");
        let profile_2 = DbActorProfile::remote_for_test(
            "user2",
            "https://test.net",
        );
        let csv = export_profiles_to_csv(
            "example.org",
            vec![profile_1, profile_2],
        );
        assert_eq!(csv, "user1@example.org\nuser2@test.net\n");
    }

    #[test]
    fn test_parse_address_list() {
        let csv = concat!(
            "\nuser1@example.net\n",
            "user2@example.com  \n",
            "@user1@example.net",
        );
        let addresses = parse_address_list(csv).unwrap();
        assert_eq!(addresses.len(), 2);
        let addresses: Vec<_> = addresses.into_iter()
            .map(|address| address.to_string())
            .collect();
        assert_eq!(addresses, vec![
            "user1@example.net",
            "user2@example.com",
        ]);
    }

    #[test]
    fn test_parse_address_list_mastodon() {
        let csv = concat!(
            "Account address,Show boosts,Notify on new posts,Languages\n",
            "user1@one.test,false,false,\n",
            "user2@two.test,true,false,\n",
        );
        let addresses = parse_address_list(csv).unwrap();
        assert_eq!(addresses.len(), 2);
        let addresses: Vec<_> = addresses.into_iter()
            .map(|address| address.to_string())
            .collect();
        assert_eq!(addresses, vec![
            "user1@one.test",
            "user2@two.test",
        ]);
    }
}
