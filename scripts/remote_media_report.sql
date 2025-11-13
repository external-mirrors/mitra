SELECT hostname, count(*), sum(file_size) / 1000000 AS size_mb
FROM (
    SELECT
        split_part(actor_profile.actor_id, '/', 3) AS hostname,
        CAST(media_attachment.media ->> 'file_size' AS INTEGER) AS file_size
    FROM media_attachment
    JOIN actor_profile ON actor_profile.id = media_attachment.owner_id
    WHERE
        actor_id IS NOT NULL
        AND media_attachment.media ->> 'file_size' IS NOT NULL
        AND media_attachment.created_at > CURRENT_TIMESTAMP - INTERVAL '3 days'
) AS media
GROUP BY hostname
ORDER BY size_mb DESC;
