CREATE TABLE IF NOT EXISTS log_daily (
    day VARCHAR(10) NOT NULL,
    user_id BIGINT NOT NULL,
    token_id BIGINT NOT NULL DEFAULT 0,
    channel_id BIGINT NOT NULL DEFAULT 0,
    model VARCHAR(255) NOT NULL,
    status VARCHAR(32) NOT NULL,
    requests BIGINT NOT NULL DEFAULT 0,
    prompt_tokens BIGINT NOT NULL DEFAULT 0,
    completion_tokens BIGINT NOT NULL DEFAULT 0,
    cache_read_tokens BIGINT NOT NULL DEFAULT 0,
    cache_write_tokens BIGINT NOT NULL DEFAULT 0,
    reasoning_tokens BIGINT NOT NULL DEFAULT 0,
    quota BIGINT NOT NULL DEFAULT 0,
    PRIMARY KEY (day, user_id, token_id, channel_id, model, status),
    KEY idx_log_daily_user_day (user_id, day)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
