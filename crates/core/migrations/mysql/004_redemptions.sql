CREATE TABLE IF NOT EXISTS redemptions (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) NOT NULL DEFAULT '',
    code VARCHAR(64) NOT NULL,
    quota BIGINT NOT NULL,
    status VARCHAR(32) NOT NULL DEFAULT 'unused',
    redeemed_by BIGINT,
    redeemed_at VARCHAR(32),
    created_at VARCHAR(32) NOT NULL,
    UNIQUE KEY uq_redemptions_code (code),
    KEY idx_redemptions_status (status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin
