use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use mitra_activitypub::authority::Authority;
use mitra_adapters::accounts::account_type_to_str;
use mitra_models::accounts::types::AccountAdminInfo;

use crate::mastodon_api::{
    accounts::types::{Account, Role},
    media_server::ClientMediaServer,
};

// https://docs.joinmastodon.org/entities/Admin_Account/
#[derive(Serialize)]
pub struct AdminAccount {
    id: Uuid,
    role: Option<Role>,
    account: Account,

    // Additional fields
    account_type: &'static str,
    last_login_at: Option<DateTime<Utc>>,
}

impl AdminAccount {
    pub fn from_db(
        authority: &Authority,
        media_server: &ClientMediaServer,
        account_info: AccountAdminInfo,
    ) -> Self {
        Self {
            id: account_info.profile.id,
            role: account_info.role.map(Role::from_db),
            account: Account::from_profile(authority, media_server, account_info.profile),
            account_type: account_type_to_str(account_info.account_type),
            last_login_at: account_info.last_login,
        }
    }
}

#[cfg(test)]
mod tests {
    use mitra_models::{
        accounts::types::{
            AccountType,
            Role as DbRole,
        },
        profiles::types::DbActorProfile,
    };
    use super::*;

    #[test]
    fn test_admin_account() {
        let authority = Authority::server_unchecked("https://social.example");
        let media_server = ClientMediaServer::for_test("/media");
        let profile = DbActorProfile::local_for_test("test");
        let account_info = AccountAdminInfo {
            account_type: AccountType::User,
            profile,
            role: Some(DbRole::NormalUser),
            last_login: None,
        };
        let admin_account = AdminAccount::from_db(
            &authority,
            &media_server,
            account_info,
        );
        assert_eq!(admin_account.account_type, "user");
        assert_eq!(admin_account.role.unwrap().name, "user");
    }
}
