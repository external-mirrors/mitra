use serde::Deserialize;

#[derive(Deserialize)]
pub struct PasswordChangeRequest {
    pub new_password: String,
}

#[derive(Deserialize)]
pub struct AddAliasRequest {
    pub acct: String,
}

#[derive(Deserialize)]
pub struct RemoveAliasRequest {
    pub actor_id: String,
}

#[derive(Deserialize)]
pub struct ImportFollowsRequest {
    pub follows_csv: String,
}

#[derive(Deserialize)]
pub struct ImportFollowersRequest {
    pub from_actor_id: String,
    pub followers_csv: String,
}

#[derive(Deserialize)]
pub struct MoveFollowersRequest {
    pub target_acct: String,
}
