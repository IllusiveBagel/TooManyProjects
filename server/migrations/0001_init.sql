-- Phase 0 schema: accounts, plus the core project, canvas and task tables
-- that phases 1 and 2 build on. Ids are UUIDv7 strings, times are RFC 3339 UTC.

-- Accounts --------------------------------------------------------------

CREATE TABLE users (
    id            TEXT PRIMARY KEY NOT NULL,
    email         TEXT NOT NULL UNIQUE COLLATE NOCASE,
    name          TEXT NOT NULL,
    password_hash TEXT NOT NULL,
    avatar        TEXT,
    -- 'owner' is the first account on the install; everyone else is 'member'.
    role          TEXT NOT NULL DEFAULT 'member' CHECK (role IN ('owner', 'member')),
    theme_id      TEXT NOT NULL DEFAULT 'workshop',
    theme_mode    TEXT NOT NULL DEFAULT 'system' CHECK (theme_mode IN ('system', 'dark', 'light')),
    created_at    TEXT NOT NULL,
    updated_at    TEXT NOT NULL
);

CREATE TABLE sessions (
    -- SHA-256 of the cookie value; the raw token never touches the database.
    token_hash   TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at   TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    expires_at   TEXT NOT NULL,
    user_agent   TEXT
);
CREATE INDEX sessions_user ON sessions (user_id);

CREATE TABLE invites (
    id          TEXT PRIMARY KEY NOT NULL,
    token_hash  TEXT NOT NULL UNIQUE,
    created_by  TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- 'server' = join the install only. 'account' and 'project' arrive with sharing.
    scope       TEXT NOT NULL DEFAULT 'server' CHECK (scope IN ('server', 'account', 'project')),
    project_id  TEXT REFERENCES projects (id) ON DELETE CASCADE,
    role        TEXT NOT NULL DEFAULT 'viewer' CHECK (role IN ('viewer', 'editor')),
    note        TEXT,
    expires_at  TEXT NOT NULL,
    accepted_by TEXT REFERENCES users (id) ON DELETE SET NULL,
    accepted_at TEXT,
    revoked_at  TEXT,
    created_at  TEXT NOT NULL
);

CREATE TABLE shares (
    id               TEXT PRIMARY KEY NOT NULL,
    -- Exactly one of owner_id (whole account) or project_id (one project).
    owner_id         TEXT REFERENCES users (id) ON DELETE CASCADE,
    project_id       TEXT REFERENCES projects (id) ON DELETE CASCADE,
    grantee_id       TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    role             TEXT NOT NULL CHECK (role IN ('viewer', 'editor')),
    include_children INTEGER NOT NULL DEFAULT 0,
    include_notes    INTEGER NOT NULL DEFAULT 0,
    created_at       TEXT NOT NULL,
    CHECK ((owner_id IS NULL) <> (project_id IS NULL))
);
CREATE INDEX shares_grantee ON shares (grantee_id);

-- Projects and links -----------------------------------------------------

CREATE TABLE projects (
    id          TEXT PRIMARY KEY NOT NULL,
    owner_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    title       TEXT NOT NULL,
    summary     TEXT,
    description TEXT,
    status      TEXT NOT NULL DEFAULT 'idea' CHECK (status IN ('idea', 'active', 'paused', 'done', 'archived')),
    -- A theme slot ('slot-1' .. 'slot-8') or a one-off '#rrggbb'.
    color       TEXT NOT NULL DEFAULT 'slot-1',
    icon        TEXT,
    cover_image TEXT,
    started_at  TEXT,
    target_date TEXT,
    archived_at TEXT,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX projects_owner ON projects (owner_id);

CREATE TABLE project_links (
    id         TEXT PRIMARY KEY NOT NULL,
    source_id  TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    target_id  TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    kind       TEXT NOT NULL CHECK (kind IN ('spawned', 'related', 'blocks')),
    label      TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (source_id, target_id, kind),
    CHECK (source_id <> target_id)
);
CREATE INDEX project_links_target ON project_links (target_id);

-- Canvas ----------------------------------------------------------------

CREATE TABLE canvases (
    id         TEXT PRIMARY KEY NOT NULL,
    -- Each person arranges their own canvas.
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX canvases_user ON canvases (user_id);

CREATE TABLE canvas_nodes (
    id         TEXT PRIMARY KEY NOT NULL,
    canvas_id  TEXT NOT NULL REFERENCES canvases (id) ON DELETE CASCADE,
    -- 'project' (project_id set), 'note' (note_id set, phase 3) or 'group' (a frame).
    kind       TEXT NOT NULL CHECK (kind IN ('project', 'note', 'group')),
    project_id TEXT REFERENCES projects (id) ON DELETE CASCADE,
    note_id    TEXT,
    label      TEXT,
    color      TEXT,
    x          REAL NOT NULL,
    y          REAL NOT NULL,
    width      REAL,
    height     REAL,
    collapsed  INTEGER NOT NULL DEFAULT 0,
    updated_at TEXT NOT NULL,
    UNIQUE (canvas_id, project_id)
);

-- Tasks and boards --------------------------------------------------------

CREATE TABLE statuses (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    -- Places the column's tasks on the global board.
    category   TEXT NOT NULL CHECK (category IN ('todo', 'in_progress', 'blocked', 'done')),
    color      TEXT,
    position   TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX statuses_project ON statuses (project_id, position);

CREATE TABLE tasks (
    id           TEXT PRIMARY KEY NOT NULL,
    project_id   TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    status_id    TEXT NOT NULL REFERENCES statuses (id) ON DELETE RESTRICT,
    title        TEXT NOT NULL,
    description  TEXT,
    priority     TEXT CHECK (priority IN ('low', 'medium', 'high', 'urgent')),
    due_date     TEXT,
    -- Fractional index: a drag rewrites one row.
    position     TEXT NOT NULL,
    assignee_id  TEXT REFERENCES users (id) ON DELETE SET NULL,
    completed_at TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX tasks_project_status ON tasks (project_id, status_id, position);
CREATE INDEX tasks_assignee ON tasks (assignee_id);

CREATE TABLE labels (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    color      TEXT,
    UNIQUE (project_id, name)
);

CREATE TABLE task_labels (
    task_id  TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    label_id TEXT NOT NULL REFERENCES labels (id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, label_id)
);

CREATE TABLE activity (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT REFERENCES projects (id) ON DELETE CASCADE,
    task_id    TEXT REFERENCES tasks (id) ON DELETE SET NULL,
    user_id    TEXT REFERENCES users (id) ON DELETE SET NULL,
    kind       TEXT NOT NULL,
    payload    TEXT NOT NULL DEFAULT '{}',
    -- 'web' or 'ai' (MCP), so changes an assistant made can be shown as such.
    via        TEXT NOT NULL DEFAULT 'web',
    created_at TEXT NOT NULL
);
CREATE INDEX activity_project_time ON activity (project_id, created_at);

-- Per-project card statistics for the canvas.
CREATE VIEW project_stats AS
SELECT
    p.id AS project_id,
    COUNT(t.id) AS total_tasks,
    COALESCE(SUM(CASE WHEN s.category <> 'done' THEN 1 ELSE 0 END), 0) AS open_tasks,
    COALESCE(SUM(CASE WHEN s.category = 'in_progress' THEN 1 ELSE 0 END), 0) AS in_progress_tasks,
    COALESCE(SUM(CASE WHEN s.category <> 'done' AND t.due_date IS NOT NULL
                       AND t.due_date < strftime('%Y-%m-%d', 'now') THEN 1 ELSE 0 END), 0) AS overdue_tasks,
    (SELECT COUNT(*) FROM project_links l WHERE l.source_id = p.id AND l.kind = 'spawned') AS child_count,
    (SELECT MAX(a.created_at) FROM activity a WHERE a.project_id = p.id) AS last_activity_at
FROM projects p
LEFT JOIN tasks t ON t.project_id = p.id
LEFT JOIN statuses s ON s.id = t.status_id
GROUP BY p.id;
