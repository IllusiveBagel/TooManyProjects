-- Refresh tokens that have already been swapped for new ones. If one comes
-- back, someone has a copy of it, so the whole connection is ended
-- (OAuth 2.0 Security BCP, refresh token rotation). Kept until the token
-- would have expired anyway.
CREATE TABLE oauth_spent_refresh (
    token_hash    TEXT PRIMARY KEY,
    connection_id TEXT NOT NULL REFERENCES oauth_connections(id) ON DELETE CASCADE,
    expires_at    TEXT NOT NULL
);
CREATE INDEX oauth_spent_refresh_connection ON oauth_spent_refresh(connection_id);
