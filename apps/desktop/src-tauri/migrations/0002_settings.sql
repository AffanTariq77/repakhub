-- App settings as one JSON document per key. Today only key 'app' (see settings.rs).
CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);
