ALTER TABLE media_attachment ADD COLUMN media JSONB NOT NULL DEFAULT '{}';

UPDATE media_attachment
SET media = jsonb_build_object(
    'file_name', file_name,
    'file_size', file_size,
    'digest', CASE
        WHEN digest IS NULL THEN NULL
        ELSE ARRAY[
            get_byte(digest, 0),
            get_byte(digest, 1),
            get_byte(digest, 2),
            get_byte(digest, 3),
            get_byte(digest, 4),
            get_byte(digest, 5),
            get_byte(digest, 6),
            get_byte(digest, 7),
            get_byte(digest, 8),
            get_byte(digest, 9),
            get_byte(digest, 10),
            get_byte(digest, 11),
            get_byte(digest, 12),
            get_byte(digest, 13),
            get_byte(digest, 14),
            get_byte(digest, 15),
            get_byte(digest, 16),
            get_byte(digest, 17),
            get_byte(digest, 18),
            get_byte(digest, 19),
            get_byte(digest, 20),
            get_byte(digest, 21),
            get_byte(digest, 22),
            get_byte(digest, 23),
            get_byte(digest, 24),
            get_byte(digest, 25),
            get_byte(digest, 26),
            get_byte(digest, 27),
            get_byte(digest, 28),
            get_byte(digest, 29),
            get_byte(digest, 30),
            get_byte(digest, 31)
        ]
        END,
    'media_type', media_type,
    'url', url
);

ALTER TABLE media_attachment ALTER COLUMN media DROP DEFAULT;
ALTER TABLE media_attachment DROP COLUMN file_name;
ALTER TABLE media_attachment DROP COLUMN file_size;
ALTER TABLE media_attachment DROP COLUMN digest;
ALTER TABLE media_attachment DROP COLUMN media_type;
ALTER TABLE media_attachment DROP COLUMN url;
