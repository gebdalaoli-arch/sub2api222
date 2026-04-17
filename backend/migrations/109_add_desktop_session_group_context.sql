ALTER TABLE desktop_sessions
    ADD COLUMN IF NOT EXISTS group_id BIGINT;

CREATE INDEX IF NOT EXISTS idx_desktop_sessions_group_id ON desktop_sessions(group_id);

UPDATE desktop_sessions
SET status = 'revoked',
    revoked_at = COALESCE(revoked_at, NOW()),
    updated_at = NOW()
WHERE group_id IS NULL
  AND status = 'active';
