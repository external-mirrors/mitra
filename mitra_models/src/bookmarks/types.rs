use tokio_postgres::Row;

use crate::{
    database::DatabaseError,
    posts::types::Post,
};

pub struct BookmarkedPost {
    pub bookmark_id: i32,
    pub post: Post,
}

impl TryFrom<&Row> for BookmarkedPost {
    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let bookmark_id: i32 = row.try_get("id")?;
        let post = Post::try_from(row)?;
        Ok(Self { bookmark_id, post })
    }
}
