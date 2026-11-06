-- Phase 3a: the notes vault. Notes are markdown files under
-- <data_dir>/vaults/<user id>/; these tables are only an index of them and
-- can be rebuilt from the files at any time.

CREATE TABLE notes (
    id          TEXT PRIMARY KEY,
    user_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Relative to the vault root, with forward slashes.
    path        TEXT NOT NULL,
    title       TEXT NOT NULL,
    -- First path segment: Inbox, Knowledge, Projects or anything else.
    folder      TEXT NOT NULL,
    tags        TEXT NOT NULL DEFAULT '[]',
    excerpt     TEXT NOT NULL DEFAULT '',
    -- Change detection for the vault scan.
    file_size   INTEGER NOT NULL,
    file_mtime  INTEGER NOT NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL,
    UNIQUE (user_id, path)
);
CREATE INDEX notes_user_updated ON notes (user_id, updated_at);
CREATE INDEX notes_user_title ON notes (user_id, title COLLATE NOCASE);

-- Projects a note belongs to (its `projects` front matter).
CREATE TABLE note_projects (
    note_id    TEXT NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    PRIMARY KEY (note_id, project_id)
);
CREATE INDEX note_projects_project ON note_projects (project_id);

-- Every [[wiki link]] in a note's body, by the title it names. Resolved when
-- read, so a link to a note that doesn't exist yet starts working once it does.
CREATE TABLE note_links (
    note_id      TEXT NOT NULL REFERENCES notes (id) ON DELETE CASCADE,
    -- 'note' for [[Title]], 'project' for [[@Title]].
    target_kind  TEXT NOT NULL CHECK (target_kind IN ('note', 'project')),
    target_title TEXT NOT NULL COLLATE NOCASE
);
CREATE INDEX note_links_note ON note_links (note_id);
CREATE INDEX note_links_target ON note_links (target_kind, target_title);

CREATE VIRTUAL TABLE notes_fts USING fts5 (
    note_id UNINDEXED,
    title,
    body,
    tokenize = 'porter unicode61'
);
