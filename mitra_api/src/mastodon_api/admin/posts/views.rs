use actix_web::{
    delete,
    web,
    HttpResponse,
    Scope,
};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use uuid::Uuid;

use mitra_activitypub::{
    adapters::posts::delete_local_post,
};
use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DatabaseConnectionPool},
    posts::queries::{delete_post, get_post_by_id},
    users::types::Permission,
};

use crate::mastodon_api::{
    auth::get_current_user,
    errors::MastodonError,
};

#[delete("/{post_id}")]
async fn delete_post_view(
    auth: BearerAuth,
    config: web::Data<Config>,
    db_pool: web::Data<DatabaseConnectionPool>,
    post_id: web::Path<Uuid>,
) -> Result<HttpResponse, MastodonError> {
    let db_client = &mut **get_database_client(&db_pool).await?;
    let current_user = get_current_user(db_client, auth.token()).await?;
    if !current_user.role.has_permission(Permission::DeleteAnyPost) {
        return Err(MastodonError::PermissionError);
    };
    let post = get_post_by_id(db_client, *post_id).await?;
    if post.is_local() {
        delete_local_post(
            &config,
            db_client,
            &post,
        ).await?;
    } else {
        let deletion_queue = delete_post(db_client, post.id).await?;
        deletion_queue.into_job(db_client).await?;
    };
    let empty = serde_json::json!({});
    Ok(HttpResponse::NoContent().json(empty))
}

pub fn admin_post_api_scope() -> Scope {
    web::scope("/v1/admin/posts")
        .service(delete_post_view)
}
