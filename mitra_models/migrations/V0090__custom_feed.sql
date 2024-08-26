CREATE TABLE custom_feed (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    owner_id UUID NOT NULL REFERENCES user_account (id) ON DELETE CASCADE,
    feed_name VARCHAR(200) NOT NULL,
    UNIQUE (owner_id, feed_name)
);

CREATE TABLE custom_feed_source (
    feed_id INTEGER NOT NULL REFERENCES custom_feed (id) ON DELETE CASCADE,
    source_id UUID NOT NULL REFERENCES actor_profile (id) ON DELETE CASCADE,
    PRIMARY KEY (feed_id, source_id)
);
