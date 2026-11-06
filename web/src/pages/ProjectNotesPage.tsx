import { useNavigate } from "react-router";
import { Button, ErrorNote } from "../components/ui";
import { useCreateNote, useProjectNotes } from "../notes/queries";
import { NoteCard } from "./NotesPage";
import { useProject } from "./ProjectPage";

/** A project's notes, plus notes elsewhere that mention it. */
export function ProjectNotesPage() {
  const { detail } = useProject();
  const { project } = detail;
  const notes = useProjectNotes(project.id);
  const create = useCreateNote();
  const navigate = useNavigate();

  const newNote = () =>
    create.mutate({ project_ids: [project.id] }, { onSuccess: (n) => navigate(`/notes/${n.note.id}?edit=1`) });

  return (
    <div className="flex flex-col gap-5 px-4 py-4 sm:px-5">
      <div className="flex flex-wrap items-center gap-3">
        <p className="text-text-quiet min-w-0 flex-1 text-sm">
          Notes filed under this project live in <span className="font-mono text-[12px]">Projects/{project.title}/</span> in your vault.
          Mention it from any note with <span className="font-mono text-[12px]">[[@{project.title}]]</span>.
        </p>
        <Button variant="primary" className="min-h-10 px-3 text-sm" onClick={newNote} disabled={create.isPending}>
          + Note
        </Button>
      </div>
      <ErrorNote error={notes.error ?? create.error} />
      {!notes.data ? (
        <p className="label-mono text-text-quiet">Loading…</p>
      ) : (
        <>
          <section className="flex flex-col gap-2" aria-label="Project notes">
            <h2 className="label-mono text-text-quiet text-[11px] font-normal">
              Notes <span className="text-accent">{notes.data.notes.length}</span>
            </h2>
            {notes.data.notes.length === 0 ? (
              <p className="border-rule text-text-quiet rounded-md border-dashed p-4 text-sm">No notes for this project yet.</p>
            ) : (
              <ul className="grid grid-cols-1 gap-3 lg:grid-cols-2 2xl:grid-cols-3">
                {notes.data.notes.map((n) => (
                  <li key={n.id} className="min-w-0"><NoteCard note={n} /></li>
                ))}
              </ul>
            )}
          </section>
          {notes.data.mentions.length > 0 && (
            <section className="flex flex-col gap-2" aria-label="Mentioned in">
              <h2 className="label-mono text-text-quiet text-[11px] font-normal">
                Mentioned in <span className="text-accent">{notes.data.mentions.length}</span>
              </h2>
              <ul className="grid grid-cols-1 gap-3 lg:grid-cols-2 2xl:grid-cols-3">
                {notes.data.mentions.map((n) => (
                  <li key={n.id} className="min-w-0"><NoteCard note={n} /></li>
                ))}
              </ul>
            </section>
          )}
        </>
      )}
    </div>
  );
}
