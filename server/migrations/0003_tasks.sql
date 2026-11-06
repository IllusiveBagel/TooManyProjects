-- Phase 2: task numbers (T-12), checklists, and the index the weekly chart uses.

ALTER TABLE projects ADD COLUMN next_task_number INTEGER NOT NULL DEFAULT 1;
ALTER TABLE tasks ADD COLUMN number INTEGER NOT NULL DEFAULT 0;

CREATE TABLE task_checklist (
    id         TEXT PRIMARY KEY NOT NULL,
    task_id    TEXT NOT NULL REFERENCES tasks (id) ON DELETE CASCADE,
    text       TEXT NOT NULL,
    done       INTEGER NOT NULL DEFAULT 0,
    position   TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX task_checklist_task ON task_checklist (task_id, position);

CREATE INDEX tasks_completed ON tasks (project_id, completed_at);
CREATE INDEX activity_task ON activity (task_id, created_at);
