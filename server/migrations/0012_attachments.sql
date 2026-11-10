-- Phase 4b: photos and files attached to projects, tasks, comments, database
-- rows and notes.

CREATE TABLE attachments (
    id         TEXT PRIMARY KEY NOT NULL,
    -- The project it belongs to. Only a note's files have none: they follow
    -- the note's own access.
    project_id TEXT REFERENCES projects (id) ON DELETE CASCADE,
    -- What it is attached to. Removing a task, comment or row keeps the file
    -- on the project (or the task), so a build log never loses photos.
    task_id    TEXT REFERENCES tasks (id) ON DELETE SET NULL,
    comment_id TEXT REFERENCES task_comments (id) ON DELETE SET NULL,
    row_id     TEXT REFERENCES db_rows (id) ON DELETE SET NULL,
    -- No foreign key, as with task_notes: the notes table is an index of the
    -- vault and can be rebuilt.
    note_id    TEXT,
    -- Whose storage holds the file: the project's owner, or the note's.
    owner_id   TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- Relative to the data folder: files/<owner>/<yyyy-mm>/<id>.<ext>, or
    -- vaults/<owner>/attachments/<name> for a note's file.
    path       TEXT NOT NULL,
    name       TEXT NOT NULL,
    mime       TEXT NOT NULL,
    size       INTEGER NOT NULL,
    width      INTEGER,
    height     INTEGER,
    -- When the photo was taken, from its EXIF data (local time, no zone).
    taken_at   TEXT,
    sha256     TEXT NOT NULL,
    caption    TEXT,
    -- 1 when the 400 px and 1600 px previews exist.
    has_thumbs INTEGER NOT NULL DEFAULT 0,
    added_by   TEXT REFERENCES users (id) ON DELETE SET NULL,
    via        TEXT NOT NULL DEFAULT 'web',
    created_at TEXT NOT NULL
);
CREATE INDEX attachments_project ON attachments (project_id, created_at);
CREATE INDEX attachments_task ON attachments (task_id);
CREATE INDEX attachments_comment ON attachments (comment_id);
CREATE INDEX attachments_row ON attachments (row_id);
CREATE INDEX attachments_note ON attachments (note_id);

-- Files whose record is gone, waiting to be removed from disk. A trigger
-- fills it, so cascades (a deleted project or account) clean up too.
CREATE TABLE attachment_trash (
    id   TEXT PRIMARY KEY NOT NULL,
    path TEXT NOT NULL
);
CREATE TRIGGER attachments_gone AFTER DELETE ON attachments BEGIN
    INSERT OR IGNORE INTO attachment_trash (id, path) VALUES (OLD.id, OLD.path);
END;

-- The photo on the project's canvas card.
ALTER TABLE projects ADD COLUMN cover_id TEXT REFERENCES attachments (id) ON DELETE SET NULL;
