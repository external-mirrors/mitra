ALTER TABLE follow_request ADD COLUMN has_deprecated_ap_id BOOLEAN NOT NULL DEFAULT FALSE;
UPDATE follow_request SET has_deprecated_ap_id = TRUE;

ALTER TABLE post ADD COLUMN repost_has_deprecated_ap_id BOOLEAN NOT NULL DEFAULT FALSE;
UPDATE post SET repost_has_deprecated_ap_id = TRUE;

ALTER TABLE post_reaction ADD COLUMN has_deprecated_ap_id BOOLEAN NOT NULL DEFAULT FALSE;
UPDATE post_reaction SET has_deprecated_ap_id = TRUE;
