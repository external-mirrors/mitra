use minijinja::Environment;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
#[error("template rendering error")]
pub struct TemplateError;

pub fn render_template(
    template: &str,
    context: impl Serialize,
) -> Result<String, TemplateError> {
    let env = Environment::new();
    // .html extension enables HTML escaping
    // https://docs.rs/minijinja/2.12.0/minijinja/fn.default_auto_escape_callback.html
    env.render_named_str("template.html", template, context)
        .map_err(|_| TemplateError)
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use super::*;

    #[test]
    fn test_render_template_escape_html() {
        let template = r#"<meta property="test" content="{{ value }}">"#;
        let context = json!({"value": r#"test"><script>"#});
        let output = render_template(template, context).unwrap();
        assert_eq!(
            output,
            r#"<meta property="test" content="test&quot;&gt;&lt;script&gt;">"#,
        );
    }
}
