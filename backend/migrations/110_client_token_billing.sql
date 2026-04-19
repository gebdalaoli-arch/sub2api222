SET LOCAL lock_timeout = '5s';
SET LOCAL statement_timeout = '10min';

ALTER TABLE channels
    ADD COLUMN IF NOT EXISTS settlement_unit VARCHAR(20) NOT NULL DEFAULT 'money',
    ADD COLUMN IF NOT EXISTS token_input_ratio_milli BIGINT NOT NULL DEFAULT 1000,
    ADD COLUMN IF NOT EXISTS token_output_ratio_milli BIGINT NOT NULL DEFAULT 1000,
    ADD COLUMN IF NOT EXISTS token_cache_write_ratio_milli BIGINT NOT NULL DEFAULT 1000,
    ADD COLUMN IF NOT EXISTS token_cache_read_ratio_milli BIGINT NOT NULL DEFAULT 1000;

COMMENT ON COLUMN channels.settlement_unit IS '结算单位：money=原版金额钱包，token=客户端 Token 钱包';
COMMENT ON COLUMN channels.token_input_ratio_milli IS '输入 token 扣费倍率，单位 milli-token / token';
COMMENT ON COLUMN channels.token_output_ratio_milli IS '输出 token 扣费倍率，单位 milli-token / token';
COMMENT ON COLUMN channels.token_cache_write_ratio_milli IS '缓存写入 token 扣费倍率，单位 milli-token / token';
COMMENT ON COLUMN channels.token_cache_read_ratio_milli IS '缓存读取 token 扣费倍率，单位 milli-token / token';

CREATE TABLE IF NOT EXISTS client_token_wallets (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    balance_milli_tokens BIGINT NOT NULL DEFAULT 0,
    total_recharged_milli_tokens BIGINT NOT NULL DEFAULT 0,
    total_consumed_milli_tokens BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE UNIQUE INDEX IF NOT EXISTS idx_client_token_wallets_user_channel
    ON client_token_wallets (user_id, channel_id);
CREATE INDEX IF NOT EXISTS idx_client_token_wallets_user_id
    ON client_token_wallets (user_id);

CREATE TABLE IF NOT EXISTS client_token_wallet_ledgers (
    id BIGSERIAL PRIMARY KEY,
    user_id BIGINT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    channel_id BIGINT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    source_type VARCHAR(50) NOT NULL,
    source_id VARCHAR(255) NOT NULL DEFAULT '',
    credit_milli_tokens BIGINT NOT NULL DEFAULT 0,
    debit_milli_tokens BIGINT NOT NULL DEFAULT 0,
    balance_after_milli_tokens BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_client_token_wallet_ledgers_user_id
    ON client_token_wallet_ledgers (user_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_client_token_wallet_ledgers_channel_id
    ON client_token_wallet_ledgers (channel_id, created_at DESC);

COMMENT ON TABLE client_token_wallets IS '桌面客户端专用 Token 钱包，按渠道隔离';
COMMENT ON TABLE client_token_wallet_ledgers IS '桌面客户端专用 Token 钱包流水';
