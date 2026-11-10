-- Phase 4c: full-text search over tasks and task comments, for the Ctrl+K
-- palette and the MCP search tool. Notes already have notes_fts. Triggers keep
-- both tables in step with the rows they index.

CREATE VIRTUAL TABLE tasks_fts USING fts5 (
    task_id UNINDEXED,
    title,
    description,
    tokenize = 'porter unicode61'
);
INSERT INTO tasks_fts (task_id, title, description)
    SELECT id, title, COALESCE(description, '') FROM tasks;

CREATE TRIGGER tasks_fts_insert AFTER INSERT ON tasks BEGIN
    INSERT INTO tasks_fts (task_id, title, description) VALUES (NEW.id, NEW.title, COALESCE(NEW.description, ''));
END;
CREATE TRIGGER tasks_fts_update AFTER UPDATE OF title, description ON tasks BEGIN
    DELETE FROM tasks_fts WHERE task_id = OLD.id;
    INSERT INTO tasks_fts (task_id, title, description) VALUES (NEW.id, NEW.title, COALESCE(NEW.description, ''));
END;
CREATE TRIGGER tasks_fts_delete AFTER DELETE ON tasks BEGIN
    DELETE FROM tasks_fts WHERE task_id = OLD.id;
END;

CREATE VIRTUAL TABLE task_comments_fts USING fts5 (
    comment_id UNINDEXED,
    body,
    tokenize = 'porter unicode61'
);
INSERT INTO task_comments_fts (comment_id, body) SELECT id, body FROM task_comments;

CREATE TRIGGER task_comments_fts_insert AFTER INSERT ON task_comments BEGIN
    INSERT INTO task_comments_fts (comment_id, body) VALUES (NEW.id, NEW.body);
END;
CREATE TRIGGER task_comments_fts_update AFTER UPDATE OF body ON task_comments BEGIN
    DELETE FROM task_comments_fts WHERE comment_id = OLD.id;
    INSERT INTO task_comments_fts (comment_id, body) VALUES (NEW.id, NEW.body);
END;
CREATE TRIGGER task_comments_fts_delete AFTER DELETE ON task_comments BEGIN
    DELETE FROM task_comments_fts WHERE comment_id = OLD.id;
END;
