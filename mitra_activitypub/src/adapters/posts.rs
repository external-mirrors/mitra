use mitra_config::Config;
use mitra_models::{
    accounts::{
        queries::get_user_by_id,
        types::User,
    },
    database::{DatabaseClient, DatabaseError},
    moderation_actions::helpers::on_local_post_deleted,
    posts::{
        queries::delete_post,
        types::PostDetailed,
    },
};
use mitra_services::media::MediaServer;

use crate::{
    builders::{
        add_context_activity::sync_conversation,
        delete_note::{
            prepare_delete_note,
            prepare_delete_group_note,
        },
    },
};

pub async fn delete_local_post(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    post: &PostDetailed,
) -> Result<(), DatabaseError> {
    let instance = config.instance();
    let media_server = MediaServer::new(config);
    let author = get_user_by_id(db_client, post.author.id).await?;
    let delete_note = prepare_delete_note(
        db_client,
        &instance,
        &media_server,
        &author,
        post,
    ).await?;
    let deletion_queue = delete_post(db_client, post.id).await?;
    deletion_queue.into_job(db_client).await?;
    let delete_note_json = delete_note.activity().clone();
    delete_note.save_and_enqueue(db_client).await?;
    sync_conversation(
        db_client,
        &instance,
        post.expect_conversation(),
        delete_note_json,
        post.visibility,
    ).await?;
    Ok(())
}

pub async fn delete_group_post(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    post: &PostDetailed,
    moderator: &User,
) -> Result<(), DatabaseError> {
    let instance = config.instance();
    let delete_note = prepare_delete_group_note(
        db_client,
        &instance,
        moderator,
        post,
    ).await?;
    let deletion_queue = delete_post(db_client, post.id).await?;
    deletion_queue.into_job(db_client).await?;
    if post.is_local() {
        on_local_post_deleted(
            db_client,
            moderator.id,
            post.author.id,
            None, // no reason
        ).await?;
    };
    let delete_note_json = delete_note.activity().clone();
    delete_note.save_and_enqueue(db_client).await?;
    sync_conversation(
        db_client,
        &instance,
        post.expect_conversation(),
        delete_note_json,
        post.visibility,
    ).await?;
    Ok(())
}
