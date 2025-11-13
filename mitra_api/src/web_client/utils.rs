// mitra-web
const INDEX_TITLE_ELEMENT: &str = "<title>Mitra - Federated social network</title>";
const INDEX_DESCRIPTION_ELEMENT: &str = r#"<meta name="description" content="Federated social network">"#;

pub fn replace_index_metadata(
    index_html: String,
    metadata: String,
) -> String {
    index_html
        .replace(INDEX_DESCRIPTION_ELEMENT, "")
        .replace(INDEX_TITLE_ELEMENT, &metadata)
}
