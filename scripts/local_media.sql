SELECT
    unnest(array_remove(
        ARRAY[
            avatar ->> 'file_name',
            banner ->> 'file_name'
        ],
        NULL
    )) AS file_name
FROM actor_profile
WHERE user_id IS NOT NULL OR portable_user_id IS NOT NULL

UNION

SELECT file_name FROM media_attachment
JOIN actor_profile ON (media_attachment.owner_id = actor_profile.id)
WHERE actor_profile.user_id IS NOT NULL OR actor_profile.portable_user_id IS NOT NULL

UNION

SELECT image ->> 'file_name' AS file_name
FROM emoji
WHERE hostname IS NULL
;
