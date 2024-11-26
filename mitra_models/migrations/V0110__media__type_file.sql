UPDATE media_attachment
SET media = media || jsonb_build_object('type', 'file');

UPDATE actor_profile
SET avatar = avatar || jsonb_build_object('type', 'file')
WHERE avatar IS NOT NULL;

UPDATE actor_profile
SET banner = banner || jsonb_build_object('type', 'file')
WHERE banner IS NOT NULL;

UPDATE emoji
SET image = image || jsonb_build_object('type', 'file');

WITH profile_emojis AS (
    SELECT
    actor_profile.id AS profile_id,
    COALESCE(
        jsonb_agg(emoji) FILTER (WHERE emoji.id IS NOT NULL),
        '[]'
    ) AS emojis
    FROM actor_profile
    LEFT JOIN profile_emoji ON (profile_emoji.profile_id = actor_profile.id)
    LEFT JOIN emoji ON (emoji.id = profile_emoji.emoji_id)
    GROUP BY actor_profile.id
)
UPDATE actor_profile
SET emojis = profile_emojis.emojis
FROM profile_emojis
WHERE actor_profile.id = profile_emojis.profile_id;
