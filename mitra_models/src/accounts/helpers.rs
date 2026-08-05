use apx_core::crypto::{
    eddsa::Ed25519SecretKey,
    rsa::RsaSecretKey,
};
use uuid::Uuid;

use crate::{
    database::{DatabaseClient, DatabaseError},
    profiles::types::ANONYMOUS,
};

use super::{
    queries::{
        create_automated_account,
        get_user_by_id,
        get_user_by_name,
    },
    types::{
        AutomatedAccountData,
        AutomatedAccountDetailed,
        AutomatedAccountType,
        User,
    },
};

pub async fn create_anonymous_account(
    db_client: &mut impl DatabaseClient,
    rsa_secret_key: RsaSecretKey,
    ed25519_secret_key: Ed25519SecretKey,
) -> Result<AutomatedAccountDetailed, DatabaseError> {
    let account_data = AutomatedAccountData {
        username: ANONYMOUS.to_owned(),
        bio: None,
        bio_source: None,
        emojis: vec![],
        account_type: AutomatedAccountType::Anonymous,
        rsa_secret_key,
        ed25519_secret_key,
    };
    create_automated_account(db_client, account_data).await
}

pub async fn get_user_by_id_or_name(
    db_client: &impl DatabaseClient,
    user_id_or_name: &str,
) -> Result<User, DatabaseError> {
    if let Ok(user_id) = Uuid::parse_str(user_id_or_name) {
        get_user_by_id(db_client, user_id).await
    } else {
        get_user_by_name(db_client, user_id_or_name).await
    }
}
