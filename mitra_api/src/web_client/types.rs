use serde::Serialize;

#[derive(Serialize)]
pub struct MetadataBlock {
    pub title: String,
    pub title_short: String,
    pub instance_title: String,
    pub page_type: &'static str,
    pub image_url: String,
    pub atom_url: Option<String>,
}
