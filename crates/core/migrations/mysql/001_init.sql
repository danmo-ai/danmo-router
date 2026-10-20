CREATE TABLE IF NOT EXISTS users (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    username VARCHAR(191) NOT NULL,
    password_hash VARCHAR(255) NOT NULL,
    role VARCHAR(32) NOT NULL DEFAULT 'user',
    quota BIGINT NOT NULL DEFAULT 0,
    used_quota BIGINT NOT NULL DEFAULT 0,
    group_name VARCHAR(191) NOT NULL DEFAULT 'default',
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    created_at VARCHAR(32) NOT NULL,
    updated_at VARCHAR(32) NOT NULL,
    UNIQUE KEY uq_users_username (username)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE IF NOT EXISTS tokens (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id BIGINT NOT NULL,
    name VARCHAR(255) NOT NULL,
    key_hash VARCHAR(128) NOT NULL,
    key_prefix VARCHAR(32) NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'active',
    remain_quota BIGINT NOT NULL DEFAULT -1,
    unlimited_quota BIGINT NOT NULL DEFAULT 1,
    expired_at VARCHAR(32),
    models VARCHAR(4096) NOT NULL DEFAULT '',
    group_name VARCHAR(191),
    created_at VARCHAR(32) NOT NULL,
    updated_at VARCHAR(32) NOT NULL,
    UNIQUE KEY uq_tokens_key_hash (key_hash),
    KEY idx_tokens_user_id (user_id),
    CONSTRAINT fk_tokens_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE IF NOT EXISTS channels (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL,
    type VARCHAR(64) NOT NULL DEFAULT 'openai_compat',
    key_enc VARCHAR(4096) NOT NULL,
    base_url VARCHAR(1024) NOT NULL,
    models VARCHAR(2048) NOT NULL DEFAULT '',
    group_name VARCHAR(191) NOT NULL DEFAULT 'default',
    priority BIGINT NOT NULL DEFAULT 0,
    weight BIGINT NOT NULL DEFAULT 1,
    status VARCHAR(32) NOT NULL DEFAULT 'enabled',
    created_at VARCHAR(32) NOT NULL,
    updated_at VARCHAR(32) NOT NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE IF NOT EXISTS abilities (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    channel_id BIGINT NOT NULL,
    group_name VARCHAR(191) NOT NULL,
    model VARCHAR(255) NOT NULL,
    UNIQUE KEY uq_abilities (group_name, model, channel_id),
    KEY idx_abilities_channel_id (channel_id),
    CONSTRAINT fk_abilities_channel FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE IF NOT EXISTS model_prices (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    model VARCHAR(255) NOT NULL,
    model_ratio DOUBLE NOT NULL DEFAULT 1.0,
    completion_ratio DOUBLE NOT NULL DEFAULT 1.0,
    updated_at VARCHAR(32) NOT NULL,
    UNIQUE KEY uq_model_prices_model (model)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE IF NOT EXISTS group_ratios (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    group_name VARCHAR(191) NOT NULL,
    ratio DOUBLE NOT NULL DEFAULT 1.0,
    updated_at VARCHAR(32) NOT NULL,
    UNIQUE KEY uq_group_ratios_group_name (group_name)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE IF NOT EXISTS logs (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    user_id BIGINT NOT NULL,
    token_id BIGINT,
    channel_id BIGINT,
    model VARCHAR(255) NOT NULL,
    prompt_tokens BIGINT NOT NULL DEFAULT 0,
    completion_tokens BIGINT NOT NULL DEFAULT 0,
    quota BIGINT NOT NULL DEFAULT 0,
    duration_ms BIGINT NOT NULL DEFAULT 0,
    status VARCHAR(32) NOT NULL,
    estimated BIGINT NOT NULL DEFAULT 0,
    error VARCHAR(2000),
    created_at VARCHAR(32) NOT NULL,
    KEY idx_logs_user_id (user_id),
    KEY idx_logs_created_at (created_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

CREATE TABLE IF NOT EXISTS sessions (
    id VARCHAR(64) NOT NULL PRIMARY KEY,
    user_id BIGINT NOT NULL,
    expires_at VARCHAR(64) NOT NULL,
    created_at VARCHAR(32) NOT NULL,
    KEY idx_sessions_user_id (user_id),
    CONSTRAINT fk_sessions_user FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
