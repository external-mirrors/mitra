ALTER TABLE post ADD COLUMN content_source_type SMALLINT;
UPDATE post SET content_source_type = 1 WHERE content_source IS NOT NULL;
