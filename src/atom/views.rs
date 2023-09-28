use actix_web::{web, HttpResponse, Responder, Scope};

use mitra_config::Config;
use mitra_models::{
    database::{get_database_client, DbPool},
    posts::queries::get_posts_by_author,
    users::queries::get_user_by_name,
};

use crate::errors::HttpError;
use super::feeds::make_feed;

const FEED_SIZE: u16 = 10;

async fn user_feed_view(
    config: web::Data<Config>,
    db_pool: web::Data<DbPool>,
    username: web::Path<String>,
) -> Result<HttpResponse, HttpError> {
    let db_client = &**get_database_client(&db_pool).await?;
    let user = get_user_by_name(db_client, &username).await?;
    // Posts are ordered by creation date
    let posts = get_posts_by_author(
        db_client,
        &user.id,
        None, // include only public posts
        false, // exclude replies
        false, // exclude reposts
        false, // not only pinned
        false, // not only media
        None,
        FEED_SIZE,
    ).await?;
    let feed = make_feed(
        &config.instance(),
        &user.profile,
        posts,
    );
    let response = HttpResponse::Ok()
        .content_type("application/atom+xml")
        .body(feed);
    Ok(response)
}

async fn user_feed_redirect(
    username: web::Path<String>,
) -> impl Responder {
    let redirect_path = format!("/feeds/users/{}", username);
    web::Redirect::to(redirect_path).permanent()
}

pub fn atom_scope() -> Scope {
    web::scope("/feeds")
        .route("/users/{username}", web::get().to(user_feed_view))
        .route("/{username}", web::get().to(user_feed_redirect))
}
