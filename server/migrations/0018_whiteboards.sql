-- Whiteboards (T-30): freeform boards of shapes, arrows, sticky notes, text,
-- pen strokes and photos, kept with a project and shared with it.

CREATE TABLE whiteboards (
    id         TEXT PRIMARY KEY NOT NULL,
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    title      TEXT NOT NULL,
    position   TEXT NOT NULL,
    -- Goes up by one with every save. Each element remembers the number it
    -- was last saved at, so someone else's board can ask for what changed
    -- since the number it last saw.
    seq        INTEGER NOT NULL DEFAULT 0,
    created_by TEXT REFERENCES users (id) ON DELETE SET NULL,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    updated_by TEXT REFERENCES users (id) ON DELETE SET NULL
);
CREATE INDEX whiteboards_project ON whiteboards (project_id, position);

-- One row per element, so two people drawing on different things never
-- overwrite each other. A deleted element stays as a tombstone (no data) so
-- other open boards learn it went.
CREATE TABLE whiteboard_elements (
    board_id   TEXT NOT NULL REFERENCES whiteboards (id) ON DELETE CASCADE,
    -- Chosen by the browser that drew it.
    id         TEXT NOT NULL,
    -- box, ellipse, diamond, sticky, text, arrow, pen or image.
    kind       TEXT NOT NULL,
    -- The rest of the element as JSON: where it is, its colour, its words.
    data       TEXT NOT NULL,
    -- Counted up by whoever changes it. The higher version wins.
    version    INTEGER NOT NULL,
    seq        INTEGER NOT NULL,
    deleted    INTEGER NOT NULL DEFAULT 0,
    updated_by TEXT REFERENCES users (id) ON DELETE SET NULL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (board_id, id)
);
CREATE INDEX whiteboard_elements_seq ON whiteboard_elements (board_id, seq);
