use postgres_types::FromSql;
use uuid::Uuid;

#[derive(FromSql)]
#[postgres(name = "custom_feed")]
pub struct CustomFeed {
    pub id: i32,
    pub owner_id: Uuid,
    pub feed_name: String,
}
