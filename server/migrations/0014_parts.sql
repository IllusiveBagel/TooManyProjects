-- Phase 4d: parts and purchases.

-- A database made from a template keeps its name, such as `parts`, so the
-- Shopping list can find every Parts database.
ALTER TABLE databases ADD COLUMN template TEXT;

-- What a template column is for (`qty`, `unit_cost`, `supplier`, `link`,
-- `status`, `for_task`, `ordered_on`, `arrived_on`), so renaming it is safe.
ALTER TABLE db_columns ADD COLUMN role TEXT;
CREATE INDEX db_columns_role ON db_columns (database_id, role) WHERE role IS NOT NULL;
CREATE INDEX databases_template ON databases (template) WHERE template IS NOT NULL;

-- Settings for the whole install, set by the owner. `currency` is the first.
CREATE TABLE instance_settings (
    key   TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
