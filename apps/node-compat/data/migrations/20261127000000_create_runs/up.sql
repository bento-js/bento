CREATE TABLE compat_runs (
    id TEXT PRIMARY KEY NOT NULL,
    startedAt TEXT NOT NULL,
    summary TEXT NOT NULL,
    report TEXT NOT NULL
);
CREATE INDEX compat_runs_started_at ON compat_runs (startedAt DESC);
