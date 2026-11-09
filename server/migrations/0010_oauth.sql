-- Signing in to the MCP server with OAuth, so an app such as Claude connects
-- by the person pressing Allow instead of pasting a token. Only SHA-256
-- hashes of codes and tokens are kept.

-- Apps that registered themselves (dynamic client registration). Public
-- clients only: no secrets, PKCE instead.
CREATE TABLE oauth_clients (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    -- JSON array of the exact addresses a sign-in may return to.
    redirect_uris TEXT NOT NULL,
    created_at    TEXT NOT NULL
);

-- One per person and app: what shows under Connected apps.
CREATE TABLE oauth_connections (
    id            TEXT PRIMARY KEY,
    user_id       TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id     TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
    client_name   TEXT NOT NULL,
    -- Where sign-in sent the person back to, such as claude.ai.
    redirect_host TEXT NOT NULL,
    scope         TEXT NOT NULL CHECK (scope IN ('read', 'write')),
    created_at    TEXT NOT NULL,
    last_used_at  TEXT,
    revoked_at    TEXT,
    UNIQUE (user_id, client_id)
);

-- Handed back after Allow; swapped once for tokens within ten minutes.
CREATE TABLE oauth_codes (
    code_hash      TEXT PRIMARY KEY,
    user_id        TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    client_id      TEXT NOT NULL REFERENCES oauth_clients(id) ON DELETE CASCADE,
    redirect_uri   TEXT NOT NULL,
    code_challenge TEXT NOT NULL,
    scope          TEXT NOT NULL CHECK (scope IN ('read', 'write')),
    expires_at     TEXT NOT NULL
);

CREATE TABLE oauth_tokens (
    token_hash    TEXT PRIMARY KEY,
    connection_id TEXT NOT NULL REFERENCES oauth_connections(id) ON DELETE CASCADE,
    kind          TEXT NOT NULL CHECK (kind IN ('access', 'refresh')),
    expires_at    TEXT NOT NULL
);
CREATE INDEX oauth_tokens_connection ON oauth_tokens(connection_id);
