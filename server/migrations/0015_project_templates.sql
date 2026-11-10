-- Phase 4e-2: project templates.
--
-- A template is what a new project starts with: board columns, labels,
-- starter tasks, database schemas and note skeletons, kept as JSON in `body`.
-- The built-in templates (PCB, prop, car, video, software) live in the server
-- code, not here, so they improve with each release.
CREATE TABLE project_templates (
    id           TEXT PRIMARY KEY NOT NULL,
    owner_id     TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    name         TEXT NOT NULL,
    body         TEXT NOT NULL,
    -- Shown to the people the owner shares everything with.
    shared       INTEGER NOT NULL DEFAULT 0,
    -- The project it was saved from, as it was called then.
    source_title TEXT,
    created_at   TEXT NOT NULL,
    updated_at   TEXT NOT NULL
);
CREATE INDEX project_templates_owner ON project_templates (owner_id);
