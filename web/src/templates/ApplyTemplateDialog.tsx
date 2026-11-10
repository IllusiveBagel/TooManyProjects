import { useState } from "react";
import type { Project } from "../api/client";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { bringsText } from "./format";
import { useApplyTemplate, useTemplates } from "./queries";
import { TemplatePicker } from "./TemplatePicker";

/** Give a project with an empty board a template's columns and the rest. */
export function ApplyTemplateDialog({
  project,
  onClose,
}: {
  project: Project;
  onClose: () => void;
}) {
  const templates = useTemplates().data ?? [];
  const apply = useApplyTemplate();
  const [templateId, setTemplateId] = useState("");
  const chosen = templates.find((t) => t.id === templateId);
  return (
    <Modal
      wide
      title={`Use a template for ${project.title}`}
      onClose={onClose}
      actions={
        <Button type="button" className="shrink-0" onClick={onClose}>
          Cancel
        </Button>
      }
    >
      <p className="text-text-quiet mb-4 text-sm">
        The board has no tasks yet, so its columns can switch. Labels, starter
        tasks, databases and notes are added.
      </p>
      <TemplatePicker
        templates={templates}
        value={templateId}
        onChange={setTemplateId}
      />
      <ErrorNote error={apply.error} />
      <div className="mt-5 flex flex-wrap items-center gap-3">
        <p className="text-text-quiet min-w-0 flex-1 text-sm">
          {chosen && `${chosen.name}: ${bringsText(chosen)}.`}
        </p>
        <Button
          variant="primary"
          disabled={!chosen || apply.isPending}
          onClick={() =>
            apply.mutate(
              { projectId: project.id, templateId },
              { onSuccess: onClose },
            )
          }
        >
          Use this template
        </Button>
      </div>
    </Modal>
  );
}
