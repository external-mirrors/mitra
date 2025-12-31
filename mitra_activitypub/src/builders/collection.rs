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

    #[serde(skip_serializing_if = "Vec::is_empty")]
    ordered_items: Vec<JsonValue>,

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
            ordered_items: vec![],
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
            ordered_items: items,
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
            ordered_items: items,
            first: None,
            total_items: None,
        }
    }
}
