import { useState, type FormEvent } from "react";
import { Modal } from "../components/Modal";
import { ColorPicker, StatusPicker } from "../components/Pickers";
import { Button, ErrorNote, Field } from "../components/ui";
import { BLANK_ID, bringsText } from "../templates/format";
import { useTemplates } from "../templates/queries";
import { TemplatePicker } from "../templates/TemplatePicker";
import { useCreateProject } from "./queries";
import { useCanvasUi, type NewProjectDraft } from "./store";

type ProjectChoice = { id: string; title: string };
type Props = {
  parentTitle?: string;
  suggestedColor: string;
  projects: ProjectChoice[];
};

export function NewProjectDialog(props: Props) {
  const draft = useCanvasUi((s) => s.newProject);
  // A fresh form each time the dialog opens.
  return draft ? <DialogBody {...props} draft={draft} /> : null;
}

function DialogBody({
  parentTitle,
  suggestedColor,
  projects,
  draft,
}: Props & { draft: NewProjectDraft }) {
  const close = useCanvasUi((s) => s.closeNewProject);
  const openProject = useCanvasUi((s) => s.openProject);
  const create = useCreateProject();
  const templates = useTemplates().data ?? [];
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [status, setStatus] = useState("active");
  const [color, setColor] = useState(suggestedColor);
  const [parent, setParent] = useState(draft.spawnedFrom ?? "");
  const [templateId, setTemplateId] = useState(BLANK_ID);
  const chosen = templates.find((t) => t.id === templateId);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
        title,
        summary: summary || null,
        status,
        color,
        spawned_from: parent || null,
        position: draft.position ?? null,
        template_id: templateId === BLANK_ID ? null : templateId,
      },
      {
        onSuccess: (project) => {
          close();
          openProject(project.id);
        },
      },
    );
  };

  return (
    <Modal
      wide
      title={parentTitle ? `Spawn from ${parentTitle}` : "New project"}
      onClose={close}
      actions={
        <Button type="button" className="shrink-0" onClick={close}>
          Cancel
        </Button>
      }
    >
      <form onSubmit={submit} className="flex flex-col gap-5">
        {/* Phones: title, Start from, then the rest. Wider: the fields on the left, the picker on the right. */}
        <div className="grid gap-5 sm:grid-cols-[minmax(0,5fr)_minmax(0,9fr)] sm:grid-rows-[auto_1fr] sm:gap-x-7 sm:gap-y-4">
          <div className="sm:col-start-1 sm:row-start-1">
            <Field
              label="Title"
              required
              maxLength={200}
              value={title}
              onChange={(e) => setTitle(e.target.value)}
            />
          </div>
          <div className="order-last flex flex-col gap-4 sm:order-none sm:col-start-1 sm:row-start-2">
            <Field
              label="Summary (optional)"
              maxLength={500}
              value={summary}
              onChange={(e) => setSummary(e.target.value)}
            />
            {!draft.spawnedFrom && (
              <label className="flex flex-col gap-1.5">
                <span className="label-mono text-text-quiet">Spawned from</span>
                <select
                  className="border-rule bg-surface min-h-11 rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent"
                  value={parent}
                  onChange={(e) => setParent(e.target.value)}
                >
                  <option value="">None</option>
                  {projects.map((p) => (
                    <option key={p.id} value={p.id}>
                      {p.title}
                    </option>
                  ))}
                </select>
              </label>
            )}
            <ColorPicker value={color} onChange={setColor} />
            <StatusPicker value={status} onChange={setStatus} />
          </div>
          <div className="min-w-0 sm:col-start-2 sm:row-span-2 sm:row-start-1">
            {templates.length > 0 && (
              <TemplatePicker
                templates={templates}
                value={templateId}
                onChange={setTemplateId}
              />
            )}
          </div>
        </div>
        <ErrorNote error={create.error} />
        <div className="border-rule bg-surface-card sticky bottom-0 -mx-5 -mb-5 flex flex-wrap items-center gap-3 border-x-0 border-b-0 px-5 py-4 sm:-mx-7 sm:-mb-7 sm:px-7 sm:py-5">
          <p className="text-text-quiet min-w-0 flex-1 text-sm max-sm:hidden">
            {chosen &&
              `${chosen.name}: ${bringsText(chosen)}. You can change any of it later.`}
          </p>
          <Button
            variant="primary"
            type="submit"
            className="max-sm:w-full"
            disabled={create.isPending || !title.trim()}
          >
            {parentTitle ? "Spawn project" : "Create project"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
