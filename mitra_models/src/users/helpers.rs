use uuid::Uuid;

use mitra_utils::{
    crypto_eddsa::{
        generate_ed25519_key,
    },
};

use crate::{
    database::{
        DatabaseClient,
        DatabaseError,
    },
};

use super::queries::set_user_ed25519_secret_key;

pub async fn add_ed25519_keys(
    db_client: &impl DatabaseClient,
) -> Result<usize, DatabaseError> {
    let rows = db_client.query(
        "
        SELECT user_account.id
        FROM user_account
        WHERE ed25519_private_key IS NULL
        ",
        &[],
    ).await?;
    for row in &rows {
        let user_id: Uuid = row.try_get("id")?;
        let ed25519_secret_key = generate_ed25519_key();
        set_user_ed25519_secret_key(
            db_client,
            user_id,
            ed25519_secret_key,
        ).await?;
    };
    Ok(rows.len())
}
