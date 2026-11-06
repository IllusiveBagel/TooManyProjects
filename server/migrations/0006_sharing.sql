-- Phase 3c: sharing. The `shares` table has been here since phase 0; this
-- adds what invites need to carry a share, one share per person per thing,
-- and the two views every access check reads.

ALTER TABLE invites ADD COLUMN include_children INTEGER NOT NULL DEFAULT 0;
ALTER TABLE invites ADD COLUMN include_notes INTEGER NOT NULL DEFAULT 0;

CREATE UNIQUE INDEX shares_account ON shares (owner_id, grantee_id) WHERE owner_id IS NOT NULL;
CREATE UNIQUE INDEX shares_project ON shares (project_id, grantee_id) WHERE project_id IS NOT NULL;
CREATE INDEX shares_owner ON shares (owner_id);

-- Who may see each project, and as what: `owner`, `editor` or `viewer`.
-- A person gets the strongest role any of these grant:
--   * owning it;
--   * a whole-account share from its owner;
--   * a share of the project itself;
--   * a share of an ancestor with include_children, following spawned links
--     through projects with the same owner.
CREATE VIEW project_access AS
WITH RECURSIVE
family (grantee_id, project_id, role, owner_id) AS (
    SELECT s.grantee_id, s.project_id, s.role, p.owner_id
    FROM shares s JOIN projects p ON p.id = s.project_id
    WHERE s.include_children = 1
    UNION
    SELECT f.grantee_id, l.target_id, f.role, f.owner_id
    FROM family f
    JOIN project_links l ON l.source_id = f.project_id AND l.kind = 'spawned'
    JOIN projects c ON c.id = l.target_id AND c.owner_id = f.owner_id
),
grants (user_id, project_id, rank) AS (
    SELECT owner_id, id, 3 FROM projects
    UNION ALL
    SELECT s.grantee_id, p.id, CASE s.role WHEN 'editor' THEN 2 ELSE 1 END
    FROM shares s JOIN projects p ON p.owner_id = s.owner_id
    UNION ALL
    SELECT grantee_id, project_id, CASE role WHEN 'editor' THEN 2 ELSE 1 END
    FROM shares WHERE project_id IS NOT NULL
    UNION ALL
    SELECT grantee_id, project_id, CASE role WHEN 'editor' THEN 2 ELSE 1 END FROM family
)
SELECT user_id, project_id,
    CASE MAX(rank) WHEN 3 THEN 'owner' WHEN 2 THEN 'editor' ELSE 'viewer' END AS role
FROM grants
GROUP BY user_id, project_id;

-- Who may read each note, and as what. Notes follow the projects they belong
-- to; a whole-account share with include_notes adds the rest of the vault.
-- `owner` is the person whose vault holds the file.
CREATE VIEW note_access AS
WITH grants (user_id, note_id, rank) AS (
    SELECT user_id, id, 3 FROM notes
    UNION ALL
    SELECT a.user_id, np.note_id, CASE a.role WHEN 'viewer' THEN 1 ELSE 2 END
    FROM note_projects np JOIN project_access a ON a.project_id = np.project_id
    UNION ALL
    SELECT s.grantee_id, n.id, CASE s.role WHEN 'editor' THEN 2 ELSE 1 END
    FROM shares s JOIN notes n ON n.user_id = s.owner_id
    WHERE s.owner_id IS NOT NULL AND s.include_notes = 1
)
SELECT user_id, note_id,
    CASE MAX(rank) WHEN 3 THEN 'owner' WHEN 2 THEN 'editor' ELSE 'viewer' END AS role
FROM grants
GROUP BY user_id, note_id;
