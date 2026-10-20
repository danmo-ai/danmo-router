-- MySQL databases never held comma-separated channel models, so unlike sqlite/003 there are no
-- rows to split out of channels.models / model_mapping.
CREATE TABLE IF NOT EXISTS channel_models (
    id BIGINT NOT NULL AUTO_INCREMENT PRIMARY KEY,
    channel_id BIGINT NOT NULL,
    model VARCHAR(255) NOT NULL,
    upstream_model VARCHAR(255) NOT NULL DEFAULT '',
    upstream_dialect VARCHAR(64) NOT NULL DEFAULT 'standard',
    UNIQUE KEY uq_channel_models (channel_id, model),
    KEY idx_channel_models_model (model),
    CONSTRAINT fk_channel_models_channel FOREIGN KEY (channel_id) REFERENCES channels(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4 COLLATE=utf8mb4_bin;

ALTER TABLE channels ADD COLUMN default_dialect VARCHAR(64) NOT NULL DEFAULT 'standard';
ALTER TABLE channels ADD COLUMN vendor VARCHAR(64) NOT NULL DEFAULT '';
ALTER TABLE channels DROP COLUMN models;
ALTER TABLE channels DROP COLUMN model_mapping;

ALTER TABLE model_prices ADD COLUMN native_dialects VARCHAR(1024) NOT NULL DEFAULT '';
UPDATE model_prices SET native_dialects = CASE native_usage
    WHEN 'deepseek_chat' THEN '{"openai_chat":"thinking"}'
    WHEN 'kimi_chat' THEN '{"openai_chat":"kimi"}'
    WHEN 'qianfan_chat' THEN '{"openai_chat":"qianfan"}'
    ELSE ''
END;
ALTER TABLE model_prices DROP COLUMN native_usage
