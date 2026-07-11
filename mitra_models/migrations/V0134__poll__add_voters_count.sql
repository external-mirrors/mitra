ALTER TABLE poll ADD COLUMN voters_count INTEGER CHECK (voters_count >= 0);

UPDATE poll
SET voters_count = (
    SELECT count(DISTINCT voter_id)
    FROM poll_vote
    WHERE poll_id = poll.id
)
-- only local polls
WHERE EXISTS (
    SELECT 1 FROM post
    WHERE post.id = poll.id AND post.object_id IS NULL
);
