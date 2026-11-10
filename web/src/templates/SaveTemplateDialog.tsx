import { useState } from "react";
import type { Project } from "../api/client";
import { Modal } from "../components/Modal";
import { Button, ErrorNote, Field } from "../components/ui";
import { useCreateTemplate } from "./queries";

const PARTS = [
  ["columns", "Board columns", "Names, order and what each counts as"],
  ["labels", "Labels", ""],
  [
    "tasks",
    "Open tasks",
    "As starter tasks, with their checklists. Done tasks stay behind.",
  ],
  ["databases", "Databases", "Columns and views, without rows"],
  [
    "notes",
    "Note skeletons",
    "Each note's headings, without the words under them",
  ],
] as const;

type Part = (typeof PARTS)[number][0];

/** Save a project's set-up as a template for the next one. */
export function SaveTemplateDialog({
  project,
  onClose,
}: {
  project: Project;
  onClose: () => void;
}) {
  const create = useCreateTemplate();
  const [name, setName] = useState(`${project.title}-style board`);
  const [shared, setShared] = useState(false);
  const [include, setInclude] = useState<Record<Part, boolean>>({
    columns: true,
    labels: true,
    tasks: true,
    databases: true,
    notes: false,
  });
  const [saved, setSaved] = useState(false);

  if (saved) {
    return (
      <Modal title="Template saved" onClose={onClose}>
        <p className="text-text-quiet mb-4">
          “{name}” is in the Start from list when you make a new project.
          Rename, share or delete it in Settings.
        </p>
        <div className="flex justify-end">
          <Button variant="primary" onClick={onClose}>
            Done
          </Button>
        </div>
      </Modal>
    );
  }

  return (
    <Modal title="Save as template" onClose={onClose}>
      <form
        className="flex flex-col gap-4"
        onSubmit={(e) => {
          e.preventDefault();
          create.mutate(
            { name, project_id: project.id, shared, ...include },
            { onSuccess: () => setSaved(true) },
          );
        }}
      >
        <Field
          label="Name"
          required
          maxLength={80}
          value={name}
          onChange={(e) => setName(e.target.value)}
        />
        <fieldset className="flex flex-col gap-1">
          <legend className="label-mono text-text-quiet mb-1.5">
            Bring along
          </legend>
          {PARTS.map(([key, label, hint]) => (
            <label
              key={key}
              className="flex min-h-11 cursor-pointer items-start gap-3 py-1.5"
            >
              <input
                type="checkbox"
                className="accent-accent mt-1 size-4"
                checked={include[key]}
                onChange={(e) =>
                  setInclude({ ...include, [key]: e.target.checked })
                }
              />
              <span className="flex flex-col">
                <span className="font-bold">{label}</span>
                {hint && (
                  <span className="text-text-quiet text-sm">{hint}</span>
                )}
              </span>
            </label>
          ))}
        </fieldset>
        <label className="flex min-h-11 cursor-pointer items-start gap-3">
          <input
            type="checkbox"
            className="accent-accent mt-1 size-4"
            checked={shared}
            onChange={(e) => setShared(e.target.checked)}
          />
          <span className="flex flex-col">
            <span className="font-bold">Share it</span>
            <span className="text-text-quiet text-sm">
              With the people you share everything with
            </span>
          </span>
        </label>
        <ErrorNote error={create.error} />
        <div className="flex justify-end gap-2">
          <Button type="button" onClick={onClose}>
            Cancel
          </Button>
          <Button
            type="submit"
            variant="primary"
            disabled={!name.trim() || create.isPending}
          >
            Save template
          </Button>
        </div>
      </form>
    </Modal>
  );
}
