CREATE TABLE conversation (
    id UUID PRIMARY KEY,
    root_id UUID UNIQUE NOT NULL REFERENCES post (id) ON DELETE CASCADE DEFERRABLE INITIALLY DEFERRED,
    audience VARCHAR(2000)
);

INSERT INTO conversation (id, root_id)
SELECT
    gen_random_uuid(),
    post.id
FROM post WHERE in_reply_to_id IS NULL AND post.repost_of_id IS NULL;

ALTER TABLE post ADD COLUMN conversation_id UUID REFERENCES conversation (id) ON DELETE CASCADE;

WITH RECURSIVE conversation_post (conversation_id, post_id) AS (
    SELECT conversation.id, conversation.root_id
    FROM conversation
    UNION
    SELECT conversation_post.conversation_id, post.id
    FROM post
    JOIN conversation_post ON (post.in_reply_to_id = conversation_post.post_id)
)
UPDATE post
SET conversation_id = conversation_post.conversation_id
FROM conversation_post
WHERE post.id = conversation_post.post_id;

ALTER TABLE post
    ADD CONSTRAINT post_conversation_id_repost_of_id_check
    CHECK ((conversation_id IS NULL) != (repost_of_id IS NULL));
