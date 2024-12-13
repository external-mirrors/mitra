use anyhow::Error;
use clap::Parser;

use mitra_models::{
    database::DatabaseClient,
    oauth::queries::delete_oauth_tokens,
    profiles::helpers::get_profile_by_id_or_acct,
};

/// Revoke user's OAuth access tokens
#[derive(Parser)]
pub struct RevokeOauthTokens {
    id_or_name: String,
}

impl RevokeOauthTokens {
    pub async fn execute(
        &self,
        db_client: &impl DatabaseClient,
    ) -> Result<(), Error> {
        let profile = get_profile_by_id_or_acct(
            db_client,
            &self.id_or_name,
        ).await?;
        delete_oauth_tokens(db_client, profile.id).await?;
        println!("access tokens revoked");
        Ok(())
    }
}
