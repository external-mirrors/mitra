ALTER TABLE post ADD COLUMN title TEXT;

DROP INDEX post_content_tsvector_simple_index;
CREATE INDEX post_content_tsvector_simple_index ON post USING GIN (to_tsvector('simple', COALESCE(title, '') || ' ' || content));
