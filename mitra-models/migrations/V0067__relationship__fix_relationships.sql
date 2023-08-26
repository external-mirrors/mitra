-- accept pending requests where follow relationship exists
UPDATE follow_request
SET request_status = 2
WHERE
    request_status = 1
    AND EXISTS (
        SELECT 1 FROM relationship
        WHERE
            source_id = follow_request.source_id
            AND target_id = follow_request.target_id
            AND relationship_type = 1
    );

-- delete follow relationship where follow request was rejected
DELETE FROM relationship
WHERE
    relationship_type = 1
    AND EXISTS (
        SELECT 1 FROM follow_request
        WHERE
            source_id = relationship.source_id
            AND target_id = relationship.target_id
            AND request_status = 3
    );

-- update follow counters
UPDATE actor_profile
SET follower_count = (
    SELECT count(*) FROM relationship
    WHERE relationship_type = 1 AND target_id = actor_profile.id
);
UPDATE actor_profile
SET following_count = (
    SELECT count(*) FROM relationship
    WHERE relationship_type = 1 AND source_id = actor_profile.id
);

-- delete rejected requests
DELETE FROM follow_request
WHERE request_status = 3;
