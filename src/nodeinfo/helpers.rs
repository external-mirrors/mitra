use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::queries::get_post_count,
    users::queries::{
        get_admin_user,
        get_active_user_count,
        get_user_count,
    },
};
use mitra_utils::datetime::days_before_now;

use crate::activitypub::identifiers::local_actor_id;

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
    let post_count = get_post_count(db_client, true).await?;
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

pub async fn get_instance_staff(
    config: &Config,
    db_client: &impl DatabaseClient,
) -> Result<Vec<String>, DatabaseError> {
    let maybe_admin = if config.instance_staff_public {
        get_admin_user(db_client).await?
    } else {
        None
    };
    let instance_staff = match maybe_admin {
        Some(admin) => {
            let admin_actor_id = local_actor_id(
                &config.instance_url(),
                &admin.profile.username,
            );
            vec![admin_actor_id]
        },
        None => vec![],
    };
    Ok(instance_staff)
}
