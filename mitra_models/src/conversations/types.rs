use postgres_types::FromSql;
use uuid::Uuid;

pub(crate) const AP_PUBLIC: &str = "https://www.w3.org/ns/activitystreams#Public";

#[derive(Clone, FromSql)]
#[postgres(name = "conversation")]
pub struct Conversation {
    pub id: Uuid,
    pub root_id: Uuid,
    // "audience" is None when the conversation is direct,
    // or when it is limited and created by the database migration
    pub audience: Option<String>,
}

impl Conversation {
    pub fn is_public(&self) -> bool {
        self.audience.as_ref().is_some_and(|audience| audience == AP_PUBLIC)
    }
}
