CREATE INDEX post_content_tsvector_simple_index ON post USING GIN (to_tsvector('simple', content));
