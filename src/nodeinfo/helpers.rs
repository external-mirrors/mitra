use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::get_local_post_count,
    users::queries::{get_active_user_count, get_user_count},
};
use mitra_utils::datetime::days_before_now;

use super::types::{Usage, Users};

pub async fn get_usage(db_client: &impl DatabaseClient)
    -> Result<Usage, DatabaseError>
{
    let user_count = get_user_count(db_client).await?;
    let active_halfyear = get_active_user_count(
        db_client,
        days_before_now(180),
    ).await?;
    let active_month = get_active_user_count(
        db_client,
        days_before_now(30),
    ).await?;
    let post_count = get_local_post_count(db_client).await?;
    let usage = Usage {
        users: Users {
            total: user_count,
            active_halfyear,
            active_month,
        },
        local_posts: post_count,
    };
    Ok(usage)
}
