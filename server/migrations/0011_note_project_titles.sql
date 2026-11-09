-- A note's front matter names its projects by title. Until now every project
-- by that title the note's owner could see was filed, so someone could make a
-- project with the same name, share it with you, and read and edit your
-- notes. The index now prefers your own project (see vault::file_under); this
-- drops filings made the old way that the new rules would not make.
DELETE FROM note_projects
WHERE rowid IN (
    SELECT np.rowid
    FROM note_projects np
    JOIN notes n ON n.id = np.note_id
    JOIN projects p ON p.id = np.project_id
    WHERE p.owner_id <> n.user_id
      AND (
        -- The owner has a project of that name of their own.
        EXISTS (SELECT 1 FROM projects q
                WHERE q.owner_id = n.user_id AND q.title = p.title COLLATE NOCASE)
        -- Or another shared project of that name is filed too: ambiguous.
        OR EXISTS (SELECT 1 FROM note_projects np2 JOIN projects q ON q.id = np2.project_id
                   WHERE np2.note_id = np.note_id AND q.id <> p.id
                     AND q.title = p.title COLLATE NOCASE)
      )
);
