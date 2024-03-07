ALTER TABLE post_reaction ADD COLUMN content VARCHAR(102);
ALTER TABLE post_reaction ADD COLUMN emoji_id UUID REFERENCES emoji (id) ON DELETE CASCADE;
ALTER TABLE post_reaction
    ADD CONSTRAINT post_reaction_content_emoji_id_check
    CHECK (content IS NOT NULL OR emoji_id IS NULL);
