ALTER TABLE mention RENAME TO post_mention;
ALTER TABLE post_mention ADD COLUMN id INTEGER UNIQUE GENERATED ALWAYS AS IDENTITY;
