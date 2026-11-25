-- One row per installed copy of a game, per source.
-- ponytail: no separate `games` table yet; split it out when cross-source matching (same game on Steam + GOG) lands.
CREATE TABLE installations (
    id                INTEGER PRIMARY KEY,
    source            TEXT    NOT NULL,             -- adapter id, e.g. 'steam'
    source_game_id    TEXT    NOT NULL,             -- id within that source, e.g. Steam appid
    name              TEXT    NOT NULL,
    path              TEXT    NOT NULL,
    installed_version TEXT,                         -- NULL = unknown; never guessed
    version_source    TEXT,                         -- how installed_version was determined
    update_available  INTEGER,                      -- NULL = unknown, 0/1 otherwise
    size_bytes        INTEGER,
    last_updated      INTEGER,                      -- unix seconds, as reported by the source
    scanned_at        INTEGER NOT NULL,             -- unix millis of the scan that last saw it
    UNIQUE (source, source_game_id)
);
