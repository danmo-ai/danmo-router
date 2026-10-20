CREATE TABLE IF NOT EXISTS redemptions (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL DEFAULT '',
    code TEXT NOT NULL UNIQUE,
    quota INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'unused',
    redeemed_by INTEGER,
    redeemed_at TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE INDEX IF NOT EXISTS idx_redemptions_status ON redemptions(status);
