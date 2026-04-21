use serde::Serialize;
use serde_json::{Value as JsonValue};

use crate::{
    contexts::{build_default_context, Context},
    vocabulary::{ORDERED_COLLECTION, ORDERED_COLLECTION_PAGE},
};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderedCollection {
    #[serde(rename = "@context")]
    _context: Context,

    id: String,

    #[serde(rename = "type")]
    object_type: String,

    #[serde(skip_serializing_if = "Option::is_none")]
    attributed_to: Option<String>,

    // Can be serialized into an empty array
    #[serde(skip_serializing_if = "Option::is_none")]
    ordered_items: Option<Vec<JsonValue>>,

    #[serde(skip_serializing_if = "Option::is_none")]
    first: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    total_items: Option<i32>,
}

impl OrderedCollection {
    pub const PAGE_SIZE: u16 = 20;

    pub fn new(
        collection_id: String,
        first_page_id: Option<String>,
        total_items: Option<i32>,
    ) -> Self {
        Self {
            _context: build_default_context(),
            id: collection_id,
            object_type: ORDERED_COLLECTION.to_string(),
            attributed_to: None,
            ordered_items: None,
            first: first_page_id,
            total_items,
        }
    }

    pub fn new_with_items(
        collection_id: String,
        items: Vec<JsonValue>,
    ) -> Self {
        Self {
            _context: build_default_context(),
            id: collection_id,
            object_type: ORDERED_COLLECTION.to_string(),
            attributed_to: None,
            ordered_items: Some(items),
            first: None,
            total_items: None,
        }
    }

    pub fn new_page(
        collection_page_id: String,
        items: Vec<JsonValue>,
    ) -> Self {
        Self {
            _context: build_default_context(),
            id: collection_page_id,
            object_type: ORDERED_COLLECTION_PAGE.to_string(),
            attributed_to: None,
            ordered_items: Some(items),
            first: None,
            total_items: None,
        }
    }

    pub fn with_attributed_to(mut self, attributed_to: &str) -> Self {
        self.attributed_to = Some(attributed_to.to_owned());
        self
    }
}

#[cfg(test)]
mod tests {
    use serde_json::{json, to_value};
    use super::*;

    #[test]
    fn test_build_collection_root() {
        let collection_id = "https://example.social/collection";
        let collection = OrderedCollection::new(
            collection_id.to_owned(),
            None,
            Some(20),
        );
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v2",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji"
                },
            ],
            "type": "OrderedCollection",
            "id": "https://example.social/collection",
            "totalItems": 20
        });
        assert_eq!(
            to_value(collection).unwrap(),
            expected_value,
        );
    }

    #[test]
    fn test_build_collection_with_no_items() {
        let collection_id = "https://example.social/collection";
        let collection = OrderedCollection::new_with_items(
            collection_id.to_owned(),
            vec![],
        );
        let expected_value = json!({
            "@context": [
                "https://www.w3.org/ns/activitystreams",
                "https://w3id.org/security/v1",
                "https://w3id.org/security/data-integrity/v2",
                {
                    "Hashtag": "as:Hashtag",
                    "sensitive": "as:sensitive",
                    "toot": "http://joinmastodon.org/ns#",
                    "Emoji": "toot:Emoji"
                },
            ],
            "type": "OrderedCollection",
            "id": "https://example.social/collection",
            "orderedItems": []
        });
        assert_eq!(
            to_value(collection).unwrap(),
            expected_value,
        );
    }
}
