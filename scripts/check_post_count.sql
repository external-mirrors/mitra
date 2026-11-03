SELECT id
FROM (
    SELECT
        actor_profile.id,
        actor_profile.post_count,
        (SELECT count(*) FROM post WHERE post.author_id = actor_profile.id) AS post_count_real
    FROM actor_profile
) AS actor
WHERE post_count != post_count_real;
