-- Whiteboards, part 2 (T-30): whether the last change came from the web app
-- or an AI assistant, and a quick way to find the boards that link a task.
ALTER TABLE whiteboards ADD COLUMN updated_via TEXT NOT NULL DEFAULT 'web';

-- Shapes that stand for a task keep its id in their data.
CREATE INDEX whiteboard_elements_task ON whiteboard_elements (json_extract(data, '$.task'))
    WHERE deleted = 0 AND json_extract(data, '$.task') IS NOT NULL;
