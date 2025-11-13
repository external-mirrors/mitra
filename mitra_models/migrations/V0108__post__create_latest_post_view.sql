CREATE MATERIALIZED VIEW latest_post AS
    SELECT
        author_id,
        max(created_at) AS created_at
    FROM post
    GROUP BY author_id;
