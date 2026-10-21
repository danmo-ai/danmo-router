CREATE TABLE IF NOT EXISTS log_daily (
    day TEXT NOT NULL,
    user_id INTEGER NOT NULL,
    token_id INTEGER NOT NULL DEFAULT 0,
    channel_id INTEGER NOT NULL DEFAULT 0,
    model TEXT NOT NULL,
    status TEXT NOT NULL,
    requests INTEGER NOT NULL DEFAULT 0,
    prompt_tokens INTEGER NOT NULL DEFAULT 0,
    completion_tokens INTEGER NOT NULL DEFAULT 0,
    cache_read_tokens INTEGER NOT NULL DEFAULT 0,
    cache_write_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    quota INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (day, user_id, token_id, channel_id, model, status)
);

CREATE INDEX IF NOT EXISTS idx_log_daily_user_day ON log_daily(user_id, day);
