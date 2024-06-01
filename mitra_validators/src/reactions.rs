use mitra_models::reactions::types::ReactionData;
use mitra_utils::unicode::is_single_character;

use super::{
    activitypub::validate_object_id,
    emojis::{
        parse_emoji_shortcode,
        validate_emoji_name,
        EMOJI_NAME_SIZE_MAX,
    },
    errors::ValidationError,
};

const REACTION_CONTENT_SIZE_MAX: usize = EMOJI_NAME_SIZE_MAX + 2; // database column limit

pub fn validate_reaction_data(
    reaction_data: &ReactionData,
) -> Result<(), ValidationError> {
    #[allow(clippy::collapsible_else_if)]
    if let Some(ref content) = reaction_data.content {
        if content.len() > REACTION_CONTENT_SIZE_MAX {
            return Err(ValidationError("reaction content is too long"));
        };
        if !is_single_character(content) {
            if reaction_data.emoji_id.is_none() {
                return Err(ValidationError("invalid reaction content"));
            };
            let emoji_name = parse_emoji_shortcode(content)
                .ok_or(ValidationError("invalid emoji shortcode"))?;
            validate_emoji_name(emoji_name)?;
        };
    } else {
        if reaction_data.emoji_id.is_some() {
            return Err(ValidationError("custom emoji reaction without content"));
        };
    };
    if let Some(ref activity_id) = reaction_data.activity_id {
        validate_object_id(activity_id)?;
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use uuid::Uuid;
    use super::*;

    #[test]
    fn test_validate_reaction_data_like() {
        let author_id = Uuid::new_v4();
        let post_id = Uuid::new_v4();
        let reaction_data = ReactionData {
            author_id: author_id,
            post_id: post_id,
            content: None,
            emoji_id: None,
            activity_id: None,
        };
        let result = validate_reaction_data(&reaction_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_reaction_data_emoji() {
        let author_id = Uuid::new_v4();
        let post_id = Uuid::new_v4();
        let reaction_data = ReactionData {
            author_id: author_id,
            post_id: post_id,
            content: Some("❤️".to_string()),
            emoji_id: None,
            activity_id: None,
        };
        let result = validate_reaction_data(&reaction_data);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_reaction_data_custom_emoji() {
        let author_id = Uuid::new_v4();
        let post_id = Uuid::new_v4();
        let emoji_id = Uuid::new_v4();
        let reaction_data = ReactionData {
            author_id: author_id,
            post_id: post_id,
            content: Some(":blobcat:".to_string()),
            emoji_id: Some(emoji_id),
            activity_id: None,
        };
        let result = validate_reaction_data(&reaction_data);
        assert!(result.is_ok());
    }
}
