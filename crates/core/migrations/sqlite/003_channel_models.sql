CREATE TABLE IF NOT EXISTS channel_models (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    channel_id INTEGER NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    model TEXT NOT NULL,
    upstream_model TEXT NOT NULL DEFAULT '',
    upstream_dialect TEXT NOT NULL DEFAULT 'standard',
    UNIQUE(channel_id, model)
);

CREATE INDEX IF NOT EXISTS idx_channel_models_model ON channel_models(model);

INSERT OR IGNORE INTO channel_models (channel_id, model, upstream_model)
WITH RECURSIVE split(channel_id, item, rest) AS (
    SELECT id, '', models || ',' FROM channels
    UNION ALL
    SELECT channel_id,
           trim(substr(rest, 1, instr(rest, ',') - 1)),
           substr(rest, instr(rest, ',') + 1)
    FROM split WHERE rest <> ''
)
SELECT s.channel_id,
       s.item,
       COALESCE(
           CASE WHEN json_valid(c.model_mapping)
                THEN json_extract(c.model_mapping, '$."' || s.item || '"')
           END,
           '')
FROM split s JOIN channels c ON c.id = s.channel_id
WHERE s.item <> '';

UPDATE channel_models SET upstream_model = '' WHERE upstream_model = model;

ALTER TABLE channels ADD COLUMN default_dialect TEXT NOT NULL DEFAULT 'standard';
ALTER TABLE channels ADD COLUMN vendor TEXT NOT NULL DEFAULT '';
ALTER TABLE channels DROP COLUMN models;
ALTER TABLE channels DROP COLUMN model_mapping;

ALTER TABLE model_prices ADD COLUMN native_dialects TEXT NOT NULL DEFAULT '';
UPDATE model_prices SET native_dialects = CASE native_usage
    WHEN 'deepseek_chat' THEN '{"openai_chat":"thinking"}'
    WHEN 'kimi_chat' THEN '{"openai_chat":"kimi"}'
    WHEN 'qianfan_chat' THEN '{"openai_chat":"qianfan"}'
    ELSE ''
END;
ALTER TABLE model_prices DROP COLUMN native_usage;

UPDATE channel_models SET upstream_dialect = COALESCE((
    SELECT json_extract(p.native_dialects, '$.' || c.upstream_protocol)
    FROM model_prices p JOIN channels c ON c.id = channel_models.channel_id
    WHERE p.model = channel_models.model AND json_valid(p.native_dialects)
), upstream_dialect);
