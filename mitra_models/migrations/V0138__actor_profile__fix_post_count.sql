UPDATE actor_profile
SET post_count = (SELECT count(*) FROM post WHERE post.author_id = actor_profile.id);
