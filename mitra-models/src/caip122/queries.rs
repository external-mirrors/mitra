use mitra_utils::caip10::AccountId;

use crate::database::{
    catch_unique_violation,
    DatabaseClient,
    DatabaseError,
};

pub async fn is_valid_caip122_nonce(
    db_client: &impl DatabaseClient,
    account_id: &AccountId,
    nonce: &str,
) -> Result<bool, DatabaseError> {
    let result = db_client.execute(
        "
        INSERT INTO caip122_nonce (account_id, nonce)
        VALUES ($1, $2)
        ",
        &[&account_id.to_string(), &nonce],
    ).await.map_err(catch_unique_violation("nonce"));
    let is_valid = match result {
        Ok(_) => true,
        Err(DatabaseError::AlreadyExists(_)) => false,
        Err(other_error) => return Err(other_error),
    };
    Ok(is_valid)
}

#[cfg(test)]
mod tests {
    use serial_test::serial;
    use crate::database::test_utils::create_test_database;
    use super::*;

    #[tokio::test]
    #[serial]
    async fn test_is_valid_caip122_nonce() {
        let db_client = &create_test_database().await;
        let account_id: AccountId =
            "eip155:31337:0xb9c5714089478a327f09197987f16f9e5d936e8a".parse().unwrap();
        let nonce = "123";
        let is_valid = is_valid_caip122_nonce(db_client, &account_id, nonce)
            .await.unwrap();
        assert_eq!(is_valid, true);
        let is_valid = is_valid_caip122_nonce(db_client, &account_id, nonce)
            .await.unwrap();
        assert_eq!(is_valid, false);
    }
}
