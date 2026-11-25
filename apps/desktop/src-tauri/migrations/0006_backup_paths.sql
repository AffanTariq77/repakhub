-- Files and folders the user chose to back up per game (JSON array of absolute paths). Survives rescans.
CREATE TABLE backup_paths (
    source         TEXT NOT NULL,
    source_game_id TEXT NOT NULL,
    paths          TEXT NOT NULL,
    PRIMARY KEY (source, source_game_id)
);
