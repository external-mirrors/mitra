// https://www.w3.org/TR/activitystreams-vocabulary/

// Activity types
pub const ACCEPT: &str = "Accept";
pub const ADD: &str = "Add";
pub const ANNOUNCE: &str = "Announce";
pub const BLOCK: &str = "Block";
pub const CREATE: &str = "Create";
pub const DELETE: &str = "Delete";
pub const DISLIKE: &str = "Dislike";
pub const EMOJI_REACT: &str = "EmojiReact";
pub const FOLLOW: &str = "Follow";
pub const LIKE: &str = "Like";
pub const LISTEN: &str = "Listen";
pub const MOVE: &str = "Move";
pub const OFFER: &str = "Offer";
pub const REJECT: &str = "Reject";
pub const REMOVE: &str = "Remove";
pub const UNDO: &str = "Undo";
pub const UPDATE: &str = "Update";

// Actor types
pub const APPLICATION: &str = "Application";
pub const GROUP: &str = "Group";
pub const PERSON: &str = "Person";
pub const SERVICE: &str = "Service";

// Object types
pub const AUDIO: &str = "Audio";
pub const DOCUMENT: &str = "Document";
pub const IMAGE: &str = "Image";
pub const NOTE: &str = "Note";
pub const QUESTION: &str = "Question";
pub const PAGE: &str = "Page";
pub const TOMBSTONE: &str = "Tombstone";
pub const VIDEO: &str = "Video";

// Link types
pub const LINK: &str = "Link";
pub const MENTION: &str = "Mention";

// Collections
pub const ORDERED_COLLECTION: &str = "OrderedCollection";
pub const ORDERED_COLLECTION_PAGE: &str = "OrderedCollectionPage";

// Valueflows
pub const AGREEMENT: &str = "Agreement";
pub const COMMITMENT: &str = "Commitment";
pub const INTENT: &str = "Intent";
pub const PROPOSAL: &str = "Proposal";

// Misc
pub const EMOJI: &str = "Emoji";
pub const HASHTAG: &str = "Hashtag";
pub const LOCK: &str = "Lock"; // Lemmy
pub const MULTIKEY: &str = "Multikey";
pub const PROPERTY_VALUE: &str = "PropertyValue";
pub const VERIFIABLE_IDENTITY_STATEMENT: &str = "VerifiableIdentityStatement";
