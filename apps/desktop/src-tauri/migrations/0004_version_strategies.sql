-- Second detection strategy: the product version embedded in the game's main executable.
ALTER TABLE installations ADD COLUMN exe_version TEXT;

-- Third strategy: a version the user entered. Kept apart from scan results so rescans never overwrite it.
CREATE TABLE version_overrides (
    source         TEXT NOT NULL,
    source_game_id TEXT NOT NULL,
    version        TEXT NOT NULL,
    PRIMARY KEY (source, source_game_id)
);
