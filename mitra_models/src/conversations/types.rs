use postgres_types::FromSql;
use uuid::Uuid;

#[derive(Clone, FromSql)]
#[postgres(name = "conversation")]
pub struct Conversation {
    pub id: Uuid,
    pub root_id: Uuid,
    // "audience" is None if conversation is public or direct,
    // and if conversation was created by database migration
    pub audience: Option<String>,
}
