-- Sharing polish (T-23, T-25).
--
-- A group copied from someone else's canvas remembers the group it came from,
-- so copying that person's layout again replaces it instead of stacking a
-- duplicate, and projects shared later land in the right copy.
ALTER TABLE canvas_nodes ADD COLUMN copied_from TEXT;
CREATE INDEX canvas_nodes_copied_from ON canvas_nodes (copied_from) WHERE copied_from IS NOT NULL;

-- Take one project away from someone who gets it through a whole-account
-- share or through a parent project. Projects spawned from it (same owner)
-- go too. A share of the project itself still counts: remove that share
-- instead.
CREATE TABLE share_exclusions (
    project_id TEXT NOT NULL REFERENCES projects (id) ON DELETE CASCADE,
    grantee_id TEXT NOT NULL REFERENCES users (id) ON DELETE CASCADE,
    created_at TEXT NOT NULL,
    PRIMARY KEY (project_id, grantee_id)
);
CREATE INDEX share_exclusions_grantee ON share_exclusions (grantee_id);

DROP VIEW note_access;
DROP VIEW project_access;

-- As in 0006, with one change: whole-account and family grants skip the
-- projects in `excluded` (an exclusion and every same-owner project spawned
-- from it).
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
excluded (grantee_id, project_id, owner_id) AS (
    SELECT e.grantee_id, e.project_id, p.owner_id
    FROM share_exclusions e JOIN projects p ON p.id = e.project_id
    UNION
    SELECT x.grantee_id, l.target_id, x.owner_id
    FROM excluded x
    JOIN project_links l ON l.source_id = x.project_id AND l.kind = 'spawned'
    JOIN projects c ON c.id = l.target_id AND c.owner_id = x.owner_id
),
grants (user_id, project_id, rank) AS (
    SELECT owner_id, id, 3 FROM projects
    UNION ALL
    SELECT s.grantee_id, p.id, CASE s.role WHEN 'editor' THEN 2 ELSE 1 END
    FROM shares s JOIN projects p ON p.owner_id = s.owner_id
    WHERE NOT EXISTS (SELECT 1 FROM excluded x WHERE x.grantee_id = s.grantee_id AND x.project_id = p.id)
    UNION ALL
    SELECT grantee_id, project_id, CASE role WHEN 'editor' THEN 2 ELSE 1 END
    FROM shares WHERE project_id IS NOT NULL
    UNION ALL
    SELECT f.grantee_id, f.project_id, CASE f.role WHEN 'editor' THEN 2 ELSE 1 END FROM family f
    WHERE NOT EXISTS (SELECT 1 FROM excluded x WHERE x.grantee_id = f.grantee_id AND x.project_id = f.project_id)
)
SELECT user_id, project_id,
    CASE MAX(rank) WHEN 3 THEN 'owner' WHEN 2 THEN 'editor' ELSE 'viewer' END AS role
FROM grants
GROUP BY user_id, project_id;

-- As in 0006, except that "share everything with notes" leaves out notes
-- filed only under projects the person was removed from.
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
      AND NOT (
          EXISTS (SELECT 1 FROM note_projects np WHERE np.note_id = n.id)
          AND NOT EXISTS (
              SELECT 1 FROM note_projects np
              JOIN project_access a ON a.project_id = np.project_id AND a.user_id = s.grantee_id
              WHERE np.note_id = n.id
          )
      )
)
SELECT user_id, note_id,
    CASE MAX(rank) WHEN 3 THEN 'owner' WHEN 2 THEN 'editor' ELSE 'viewer' END AS role
FROM grants
GROUP BY user_id, note_id;
