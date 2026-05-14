ALTER TABLE conversation ADD COLUMN is_managed BOOLEAN NOT NULL DEFAULT FALSE;
ALTER TABLE conversation ALTER COLUMN is_managed DROP DEFAULT;
ALTER TABLE conversation ADD COLUMN object_id VARCHAR(2000);

UPDATE conversation
SET is_managed = TRUE
WHERE EXISTS (
    SELECT 1
    FROM post
    WHERE
        post.id = conversation.root_id
        AND post.object_id IS NULL
);
