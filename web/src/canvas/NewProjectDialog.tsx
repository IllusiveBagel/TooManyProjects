import { useState, type FormEvent } from "react";
import { Modal } from "../components/Modal";
import { ColorPicker, StatusPicker } from "../components/Pickers";
import { Button, ErrorNote, Field } from "../components/ui";
import { useCreateProject } from "./queries";
import { useCanvasUi, type NewProjectDraft } from "./store";

type Props = { parentTitle?: string; suggestedColor: string };

export function NewProjectDialog(props: Props) {
  const draft = useCanvasUi((s) => s.newProject);
  // A fresh form each time the dialog opens.
  return draft ? <DialogBody {...props} draft={draft} /> : null;
}

function DialogBody({ parentTitle, suggestedColor, draft }: Props & { draft: NewProjectDraft }) {
  const close = useCanvasUi((s) => s.closeNewProject);
  const openProject = useCanvasUi((s) => s.openProject);
  const create = useCreateProject();
  const [title, setTitle] = useState("");
  const [summary, setSummary] = useState("");
  const [status, setStatus] = useState("active");
  const [color, setColor] = useState(suggestedColor);

  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate(
      {
        title,
        summary: summary || null,
        status,
        color,
        spawned_from: draft.spawnedFrom ?? null,
        position: draft.position ?? null,
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
    <Modal title={parentTitle ? `Spawn from ${parentTitle}` : "New project"} onClose={close}>
      <form onSubmit={submit} className="flex flex-col gap-4">
        <Field label="Title" required maxLength={200} value={title} onChange={(e) => setTitle(e.target.value)} />
        <Field
          label="One-line summary (optional)"
          maxLength={500}
          value={summary}
          onChange={(e) => setSummary(e.target.value)}
        />
        <StatusPicker value={status} onChange={setStatus} />
        <ColorPicker value={color} onChange={setColor} />
        <ErrorNote error={create.error} />
        <div className="flex justify-end gap-2">
          <Button type="button" onClick={close}>Cancel</Button>
          <Button variant="primary" type="submit" disabled={create.isPending || !title.trim()}>
            {parentTitle ? "Spawn project" : "Create project"}
          </Button>
        </div>
      </form>
    </Modal>
  );
}
