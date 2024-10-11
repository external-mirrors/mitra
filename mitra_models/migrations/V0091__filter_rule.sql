CREATE TABLE filter_rule (
    id INTEGER GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    target VARCHAR(2000) NOT NULL,
    filter_action SMALLINT NOT NULL,
    is_reversed BOOLEAN NOT NULL,
    UNIQUE (target, filter_action)
);
