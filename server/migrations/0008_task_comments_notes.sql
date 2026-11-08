-- Task comments (a running log of the work) and notes linked to tasks.

CREATE TABLE task_comments (
    id         TEXT PRIMARY KEY NOT NULL,
    task_id    TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    user_id    TEXT REFERENCES users (id) ON DELETE SET NULL,
    -- Markdown.
    body       TEXT NOT NULL,
    -- `web` or `ai`, as in the activity feed.
    via        TEXT NOT NULL DEFAULT 'web',
    created_at TEXT NOT NULL,
    edited_at  TEXT
);
CREATE INDEX task_comments_task ON task_comments (task_id, created_at);

-- A note linked to a task. `note_id` has no foreign key on purpose: the notes
-- table is only an index of the vault and can be rebuilt, while the id lives
-- on in the file's front matter. A link to a note that is no longer indexed is
-- simply not shown, and comes back if the file does.
CREATE TABLE task_notes (
    task_id    TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    note_id    TEXT NOT NULL,
    created_by TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (task_id, note_id)
);
CREATE INDEX task_notes_note ON task_notes (note_id);
