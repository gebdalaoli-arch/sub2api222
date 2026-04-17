CREATE TABLE IF NOT EXISTS desktop_sessions (
    id BIGSERIAL PRIMARY KEY,
    session_id TEXT NOT NULL UNIQUE,
    user_id BIGINT NOT NULL,
    device_id TEXT NOT NULL,
    device_name TEXT NOT NULL DEFAULT '',
    target TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    runtime_token_hash TEXT NOT NULL UNIQUE,
    profile_key TEXT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    last_seen_at TIMESTAMPTZ NOT NULL,
    revoked_at TIMESTAMPTZ NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_desktop_sessions_user_id ON desktop_sessions(user_id);
CREATE INDEX IF NOT EXISTS idx_desktop_sessions_device_id ON desktop_sessions(device_id);
CREATE INDEX IF NOT EXISTS idx_desktop_sessions_expires_at ON desktop_sessions(expires_at);
