-- Tokens for the MCP server: each belongs to one person, reads or writes as
-- them, and can be revoked. Only the SHA-256 of the secret is kept.
CREATE TABLE api_tokens (
    id           TEXT PRIMARY KEY,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    token_hash   TEXT NOT NULL UNIQUE,
    -- 'read' or 'write'
    scope        TEXT NOT NULL CHECK (scope IN ('read', 'write')),
    created_at   TEXT NOT NULL,
    last_used_at TEXT,
    revoked_at   TEXT
);
CREATE INDEX api_tokens_user ON api_tokens(user_id);
