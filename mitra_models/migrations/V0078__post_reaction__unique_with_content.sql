ALTER TABLE post_reaction DROP CONSTRAINT post_reaction_author_id_post_id_key;
ALTER TABLE post_reaction ADD CONSTRAINT post_reaction_author_id_post_id_content_key UNIQUE (author_id, post_id, content);
CREATE UNIQUE INDEX post_reaction_author_id_post_id_content_null_idx ON post_reaction (author_id, post_id) WHERE content IS NULL;
