-- The weekly calendar: work sessions on projects, busy blocks, and people
-- saying they'll join a session.
--
-- Times are kept as the wall-clock time where they were planned plus that
-- place's time zone, so "work 9 to 5 on weekdays" stays 9 to 5 across a
-- daylight-saving change. `first_start_at` and `last_end_at` are UTC and only
-- there to find the events that touch a date range quickly.

CREATE TABLE calendar_events (
    id             TEXT PRIMARY KEY NOT NULL,
    -- Who put it in the calendar.
    user_id        TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    -- `session` (time on a project, seen by everyone who can see the project)
    -- or `busy` (time you can't give to projects, seen only by you).
    kind           TEXT NOT NULL CHECK (kind IN ('session', 'busy')),
    project_id     TEXT REFERENCES projects (id) ON DELETE CASCADE,
    -- What the time is for: "Bleed the brakes", "Work".
    title          TEXT,
    -- First occurrence, local: YYYY-MM-DDTHH:MM.
    start_local    TEXT NOT NULL,
    minutes        INTEGER NOT NULL CHECK (minutes BETWEEN 15 AND 1440),
    -- IANA zone such as Europe/London.
    tz             TEXT NOT NULL,
    -- Repeats weekly on these days: bit 0 is Monday, bit 6 Sunday. 0 = once.
    repeat_days    INTEGER NOT NULL DEFAULT 0 CHECK (repeat_days BETWEEN 0 AND 127),
    -- Last day a repeat may start on, local YYYY-MM-DD. NULL = no end.
    repeat_until   TEXT,
    first_start_at TEXT NOT NULL,
    -- NULL for a repeat with no end.
    last_end_at    TEXT,
    via            TEXT NOT NULL DEFAULT 'web',
    created_at     TEXT NOT NULL,
    updated_at     TEXT NOT NULL,
    CHECK ((kind = 'session') = (project_id IS NOT NULL))
);
CREATE INDEX calendar_events_user ON calendar_events (user_id, first_start_at);
CREATE INDEX calendar_events_project ON calendar_events (project_id, first_start_at);

-- One occurrence of a repeating event taken out ("not this week").
-- `occurrence` is the local date it would have started on.
CREATE TABLE calendar_skips (
    event_id   TEXT NOT NULL REFERENCES calendar_events (id) ON DELETE CASCADE,
    occurrence TEXT NOT NULL,
    PRIMARY KEY (event_id, occurrence)
);

-- Someone saying they'll work on the project at that session too.
CREATE TABLE session_joins (
    event_id   TEXT NOT NULL REFERENCES calendar_events (id) ON DELETE CASCADE,
    occurrence TEXT NOT NULL,
    user_id    TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (event_id, occurrence, user_id)
);
CREATE INDEX session_joins_user ON session_joins (user_id);
