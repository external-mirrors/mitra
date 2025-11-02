UPDATE conversation
SET audience = 'https://www.w3.org/ns/activitystreams#Public'
WHERE EXISTS (
    SELECT 1 FROM post
    WHERE post.id = conversation.root_id
    AND post.visibility = 1
);
