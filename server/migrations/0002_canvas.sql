-- Phase 1: richer card statistics, and a canvas node per project.

DROP VIEW project_stats;

CREATE VIEW project_stats AS
SELECT
    p.id AS project_id,
    COUNT(t.id) AS total_tasks,
    COALESCE(SUM(CASE WHEN s.category = 'done' THEN 1 ELSE 0 END), 0) AS done_tasks,
    COALESCE(SUM(CASE WHEN s.category <> 'done' THEN 1 ELSE 0 END), 0) AS open_tasks,
    COALESCE(SUM(CASE WHEN s.category = 'in_progress' THEN 1 ELSE 0 END), 0) AS in_progress_tasks,
    COALESCE(SUM(CASE WHEN s.category <> 'done' AND t.due_date IS NOT NULL
                       AND t.due_date < strftime('%Y-%m-%d', 'now') THEN 1 ELSE 0 END), 0) AS overdue_tasks,
    (SELECT COUNT(*) FROM project_links l WHERE l.source_id = p.id AND l.kind = 'spawned') AS child_count,
    COALESCE(
        (SELECT MAX(a.created_at) FROM activity a WHERE a.project_id = p.id),
        p.updated_at
    ) AS last_active_at
FROM projects p
LEFT JOIN tasks t ON t.project_id = p.id
LEFT JOIN statuses s ON s.id = t.status_id
GROUP BY p.id;

CREATE INDEX canvas_nodes_project ON canvas_nodes (project_id);
CREATE INDEX project_links_source ON project_links (source_id, kind);
