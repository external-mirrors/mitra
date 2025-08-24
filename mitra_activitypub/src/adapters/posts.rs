use mitra_config::Config;
use mitra_models::{
    database::{DatabaseClient, DatabaseError},
    posts::{
        queries::delete_post,
        types::Post,
    },
    users::queries::get_user_by_id,
};
use mitra_services::media::MediaServer;

use crate::{
    builders::{
        add_context_activity::sync_conversation,
        delete_note::prepare_delete_note,
    },
};

// 1. Generate activity
// 2. Update database
// 3. Send activity
pub async fn delete_local_post(
    config: &Config,
    db_client: &mut impl DatabaseClient,
    post: &Post,
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
