CREATE UNIQUE INDEX emoji_name_hostname_null_idx ON emoji (emoji_name) WHERE hostname IS NULL;
