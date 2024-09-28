use chrono::{DateTime, Utc};
use postgres_types::FromSql;
use serde::Deserialize;
use tokio_postgres::Row;
use uuid::Uuid;

use crate::attachments::types::DbMediaAttachment;
use crate::conversations::types::Conversation;
use crate::database::{
    int_enum::{int_enum_from_sql, int_enum_to_sql},
    json_macro::json_from_sql,
    DatabaseError,
    DatabaseTypeError,
};
use crate::emojis::types::DbEmoji;
use crate::profiles::types::DbActorProfile;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Visibility {
    Public,
    Direct,
    Followers,
    Subscribers,
    Conversation,
}

impl Visibility {
    pub fn can_reply_with(self, visibility: Self, is_same_author: bool) -> bool {
        let allowed = match self {
            Self::Public => vec![
                Self::Public,
                Self::Followers,
                Self::Subscribers,
                Self::Direct,
            ],
            Self::Followers if is_same_author => vec![
                Self::Direct,
                Self::Followers,
            ],
            Self::Followers => vec![
                Self::Direct,
                Self::Conversation,
            ],
            // TODO: support subscribers conversations
            Self::Subscribers => vec![
                Self::Direct,
            ],
            Self::Conversation => vec![
                Self::Conversation,
                Self::Direct,
            ],
            Self::Direct => vec![
                Self::Direct,
            ],
        };
        allowed.contains(&visibility)
    }
}

impl Default for Visibility {
    fn default() -> Self { Self::Public }
}

impl From<Visibility> for i16 {
    fn from(value: Visibility) -> i16 {
        match value {
            Visibility::Public => 1,
            Visibility::Direct => 2,
            Visibility::Followers => 3,
            Visibility::Subscribers => 4,
            Visibility::Conversation => 5,
        }
    }
}

impl TryFrom<i16> for Visibility {
    type Error = DatabaseTypeError;

    fn try_from(value: i16) -> Result<Self, Self::Error> {
        let visibility = match value {
            1 => Self::Public,
            2 => Self::Direct,
            3 => Self::Followers,
            4 => Self::Subscribers,
            5 => Self::Conversation,
            _ => return Err(DatabaseTypeError),
        };
        Ok(visibility)
    }
}

int_enum_from_sql!(Visibility);
int_enum_to_sql!(Visibility);

#[derive(FromSql)]
#[postgres(name = "post")]
pub struct DbPost {
    pub id: Uuid,
    pub author_id: Uuid,
    pub content: String,
    pub content_source: Option<String>,
    pub conversation_id: Option<Uuid>,
    pub in_reply_to_id: Option<Uuid>,
    pub repost_of_id: Option<Uuid>,
    #[allow(dead_code)]
    repost_has_deprecated_ap_id: bool, // deprecated
    pub visibility: Visibility,
    pub is_sensitive: bool,
    pub is_pinned: bool,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub repost_count: i32,
    pub object_id: Option<String>,
    pub ipfs_cid: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>, // edited at
}

#[derive(Clone, Deserialize)]
pub struct DbPostReactions {
    pub authors: Vec<Uuid>,
    pub count: i32,
    pub content: Option<String>,
    pub emoji: Option<DbEmoji>,
}

json_from_sql!(DbPostReactions);

// List of user's actions
#[derive(Clone)]
pub struct PostActions {
    pub liked: bool,
    pub reacted_with: Vec<String>,
    pub reposted: bool,
    pub bookmarked: bool,
}

#[derive(Clone)]
pub struct Post {
    pub id: Uuid,
    pub author: DbActorProfile,
    pub content: String,
    pub content_source: Option<String>,
    pub conversation: Option<Conversation>,
    pub in_reply_to_id: Option<Uuid>,
    pub repost_of_id: Option<Uuid>,
    pub visibility: Visibility,
    pub is_sensitive: bool,
    pub is_pinned: bool,
    pub reply_count: i32,
    pub reaction_count: i32,
    pub repost_count: i32,
    pub attachments: Vec<DbMediaAttachment>,
    pub mentions: Vec<DbActorProfile>,
    pub tags: Vec<String>,
    pub links: Vec<Uuid>,
    pub emojis: Vec<DbEmoji>,
    pub reactions: Vec<DbPostReactions>,
    pub object_id: Option<String>,
    pub ipfs_cid: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,

    // These fields are not populated automatically
    // by functions in posts::queries module
    pub actions: Option<PostActions>,
    pub in_reply_to: Option<Box<Post>>,
    pub repost_of: Option<Box<Post>>,
    pub linked: Vec<Post>,
    // Might be set in get_thread
    pub parent_visible: bool,
}

impl Post {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        db_post: DbPost,
        db_author: DbActorProfile,
        db_conversation: Option<Conversation>,
        db_attachments: Vec<DbMediaAttachment>,
        db_mentions: Vec<DbActorProfile>,
        db_tags: Vec<String>,
        db_links: Vec<Uuid>,
        db_emojis: Vec<DbEmoji>,
        db_reactions: Vec<DbPostReactions>,
    ) -> Result<Self, DatabaseTypeError> {
        // Consistency checks
        if db_post.author_id != db_author.id {
            return Err(DatabaseTypeError);
        };
        if db_author.is_local() != db_post.object_id.is_none() {
            return Err(DatabaseTypeError);
        };
        if db_post.conversation_id !=
                db_conversation.as_ref().map(|conversation| conversation.id) {
            return Err(DatabaseTypeError);
        };
        if db_post.repost_of_id.is_some() && (
            db_post.content.len() != 0 ||
            db_post.content_source.is_some() ||
            db_post.is_sensitive ||
            db_post.is_pinned ||
            db_post.in_reply_to_id.is_some() ||
            db_post.ipfs_cid.is_some() ||
            db_conversation.is_some() ||
            !db_attachments.is_empty() ||
            !db_mentions.is_empty() ||
            !db_tags.is_empty() ||
            !db_links.is_empty() ||
            !db_emojis.is_empty() ||
            !db_reactions.is_empty()
        ) {
            return Err(DatabaseTypeError);
        };
        let post = Self {
            id: db_post.id,
            author: db_author,
            content: db_post.content,
            content_source: db_post.content_source,
            conversation: db_conversation,
            in_reply_to_id: db_post.in_reply_to_id,
            repost_of_id: db_post.repost_of_id,
            visibility: db_post.visibility,
            is_sensitive: db_post.is_sensitive,
            is_pinned: db_post.is_pinned,
            reply_count: db_post.reply_count,
            reaction_count: db_post.reaction_count,
            repost_count: db_post.repost_count,
            attachments: db_attachments,
            mentions: db_mentions,
            tags: db_tags,
            links: db_links,
            emojis: db_emojis,
            reactions: db_reactions,
            object_id: db_post.object_id,
            ipfs_cid: db_post.ipfs_cid,
            created_at: db_post.created_at,
            updated_at: db_post.updated_at,
            actions: None,
            in_reply_to: None,
            repost_of: None,
            linked: vec![],
            parent_visible: true,
        };
        Ok(post)
    }

    pub fn is_local(&self) -> bool {
        self.author.is_local()
    }

    pub fn is_public(&self) -> bool {
        matches!(self.visibility, Visibility::Public)
    }

    pub fn expect_conversation(&self) -> &Conversation {
        assert!(self.repost_of_id.is_none(), "should not be a repost");
        self.conversation
            .as_ref()
            .expect("conversation should not be null")
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for Post {
    fn default() -> Self {
        // TODO: use Post::new()
        let post_id = Uuid::new_v4();
        Self {
            id: post_id,
            author: DbActorProfile::default(),
            content: "".to_string(),
            content_source: None,
            conversation: Some(Conversation::for_test(post_id)),
            in_reply_to_id: None,
            repost_of_id: None,
            visibility: Visibility::Public,
            is_sensitive: false,
            is_pinned: false,
            reply_count: 0,
            reaction_count: 0,
            repost_count: 0,
            attachments: vec![],
            mentions: vec![],
            tags: vec![],
            links: vec![],
            emojis: vec![],
            reactions: vec![],
            object_id: None,
            ipfs_cid: None,
            created_at: Utc::now(),
            updated_at: None,
            actions: None,
            in_reply_to: None,
            repost_of: None,
            linked: vec![],
            parent_visible: true,
        }
    }
}

impl TryFrom<&Row> for Post {
    type Error = DatabaseError;

    fn try_from(row: &Row) -> Result<Self, Self::Error> {
        let db_post: DbPost = row.try_get("post")?;
        let db_profile: DbActorProfile = row.try_get("actor_profile")?;
        // Data from subqueries
        let db_conversation: Option<Conversation> = row.try_get("conversation")?;
        let db_attachments: Vec<DbMediaAttachment> = row.try_get("attachments")?;
        let db_mentions: Vec<DbActorProfile> = row.try_get("mentions")?;
        let db_tags: Vec<String> = row.try_get("tags")?;
        let db_links: Vec<Uuid> = row.try_get("links")?;
        let db_emojis: Vec<DbEmoji> = row.try_get("emojis")?;
        let db_reactions: Vec<DbPostReactions> = row.try_get("reactions")?;
        let post = Self::new(
            db_post,
            db_profile,
            db_conversation,
            db_attachments,
            db_mentions,
            db_tags,
            db_links,
            db_emojis,
            db_reactions,
        )?;
        Ok(post)
    }
}

pub enum PostContext {
    Top {
        audience: Option<String>,
    },
    Reply {
        conversation_id: Uuid,
        in_reply_to_id: Uuid,
    },
    Repost {
        repost_of_id: Uuid,
    },
}

impl PostContext {
    pub(super) fn in_reply_to_id(&self) -> Option<Uuid> {
        match self {
            Self::Reply { in_reply_to_id, .. } => Some(*in_reply_to_id),
            _ => None,
        }
    }

    pub(super) fn repost_of_id(&self) -> Option<Uuid> {
        match self {
            Self::Repost { repost_of_id } => Some(*repost_of_id),
            _ => None,
        }
    }
}

impl Default for PostContext {
    fn default() -> Self {
        Self::Top { audience: None }
    }
}

#[derive(Default)]
pub struct PostCreateData {
    pub context: PostContext,
    pub content: String,
    pub content_source: Option<String>,
    pub visibility: Visibility,
    pub is_sensitive: bool,
    pub attachments: Vec<Uuid>,
    pub mentions: Vec<Uuid>,
    pub tags: Vec<String>,
    pub links: Vec<Uuid>,
    pub emojis: Vec<Uuid>,
    pub object_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl PostCreateData {
    pub fn repost(
        repost_of_id: Uuid,
        object_id: Option<String>,
    ) -> Self {
        Self {
            context: PostContext::Repost { repost_of_id },
            object_id: object_id,
            created_at: Utc::now(),
            ..Default::default()
        }
    }
}

#[cfg_attr(test, derive(Default))]
pub struct PostUpdateData {
    pub content: String,
    pub content_source: Option<String>,
    pub is_sensitive: bool,
    pub attachments: Vec<Uuid>,
    pub mentions: Vec<Uuid>,
    pub tags: Vec<String>,
    pub links: Vec<Uuid>,
    pub emojis: Vec<Uuid>,
    pub updated_at: DateTime<Utc>,
}
