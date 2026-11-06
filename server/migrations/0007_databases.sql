-- Phase 3d: project databases. Tables with typed columns, rows and saved views.

CREATE TABLE databases (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    name       TEXT NOT NULL,
    position   TEXT NOT NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX databases_name ON databases (project_id, name COLLATE NOCASE);

-- `options` is JSON: select choices, a currency code, or what a relation points at.
CREATE TABLE db_columns (
    id          TEXT PRIMARY KEY NOT NULL,
    database_id TEXT NOT NULL REFERENCES databases (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    options     TEXT NOT NULL DEFAULT '{}',
    is_title    INTEGER NOT NULL DEFAULT 0,
    position    TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX db_columns_database ON db_columns (database_id, position);

-- `data` is a JSON object of column id to value. Relations live in db_relations.
CREATE TABLE db_rows (
    id          TEXT PRIMARY KEY NOT NULL,
    database_id TEXT NOT NULL REFERENCES databases (id) ON DELETE CASCADE,
    data        TEXT NOT NULL DEFAULT '{}',
    position    TEXT NOT NULL,
    created_by  TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX db_rows_database ON db_rows (database_id, position);

-- `config` is JSON: filters, sorts, hidden columns, the board's group and the calendar's date.
CREATE TABLE db_views (
    id          TEXT PRIMARY KEY NOT NULL,
    database_id TEXT NOT NULL REFERENCES databases (id) ON DELETE CASCADE,
    name        TEXT NOT NULL,
    kind        TEXT NOT NULL,
    config      TEXT NOT NULL DEFAULT '{}',
    pinned      INTEGER NOT NULL DEFAULT 0,
    position    TEXT NOT NULL,
    created_at  TEXT NOT NULL
);
CREATE INDEX db_views_database ON db_views (database_id, position);

-- A relation cell's links: to a task, note, project or a row of another database.
CREATE TABLE db_relations (
    row_id      TEXT NOT NULL REFERENCES db_rows (id) ON DELETE CASCADE,
    column_id   TEXT NOT NULL REFERENCES db_columns (id) ON DELETE CASCADE,
    target_kind TEXT NOT NULL,
    target_id   TEXT NOT NULL,
    position    INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (row_id, column_id, target_kind, target_id)
);
CREATE INDEX db_relations_target ON db_relations (target_kind, target_id);

-- A relation to something deleted goes with it.
CREATE TRIGGER db_relations_task_gone AFTER DELETE ON tasks BEGIN
    DELETE FROM db_relations WHERE target_kind = 'task' AND target_id = OLD.id;
END;
CREATE TRIGGER db_relations_note_gone AFTER DELETE ON notes BEGIN
    DELETE FROM db_relations WHERE target_kind = 'note' AND target_id = OLD.id;
END;
CREATE TRIGGER db_relations_project_gone AFTER DELETE ON projects BEGIN
    DELETE FROM db_relations WHERE target_kind = 'project' AND target_id = OLD.id;
END;
CREATE TRIGGER db_relations_row_gone AFTER DELETE ON db_rows BEGIN
    DELETE FROM db_relations WHERE target_kind = 'row' AND target_id = OLD.id;
END;
