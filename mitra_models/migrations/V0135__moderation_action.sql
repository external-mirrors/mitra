CREATE TABLE moderation_action (
    id UUID PRIMARY KEY,
    moderator_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    target_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    action_type SMALLINT NOT NULL,
    reason TEXT,
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT CURRENT_TIMESTAMP
);

ALTER TABLE notification ADD COLUMN moderation_action_id UUID REFERENCES moderation_action (id) ON DELETE CASCADE;
