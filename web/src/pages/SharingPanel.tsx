import { useState, type FormEvent } from "react";
import { Link } from "react-router";
import { Button, ErrorNote, Panel } from "../components/ui";
import { initials } from "../sharing/format";
import {
  useCreateShare,
  useDeleteShare,
  useLetBackIn,
  usePeople,
  useShares,
  useUpdateShare,
  type Share,
} from "../sharing/queries";

const selectClass =
  "border-rule bg-surface min-h-11 rounded-md px-2 font-mono text-[13px] outline-none focus:border-accent";

/** Share everything you have with someone, and see what is shared both ways. */
export function SharingPanel() {
  const people = usePeople();
  const shares = useShares();
  const create = useCreateShare();
  const update = useUpdateShare();
  const remove = useDeleteShare();
  const letBackIn = useLetBackIn();
  const [person, setPerson] = useState("");
  const [role, setRole] = useState<"viewer" | "editor">("viewer");
  const [notes, setNotes] = useState(false);

  const given = shares.data?.given ?? [];
  const received = shares.data?.received ?? [];
  const removed = shares.data?.removed ?? [];
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (person) create.mutate({ grantee_id: person, role, include_notes: notes }, { onSuccess: () => setPerson("") });
  };

  return (
    <Panel title="Sharing">
      <p className="text-text-quiet mb-4 text-sm leading-relaxed">
        Share everything you have with someone on this server: every project, its board and its notes, now and later.
        To share just one project, open it and use <b className="text-text">Share</b>; to keep one project back from
        someone you share everything with, remove them there. Each person keeps their own canvas, and your projects land
        on theirs where you keep them.
      </p>

      {(people.data?.length ?? 0) === 0 ? (
        <p className="text-text-quiet text-sm">Nobody else has an account here yet. Invite someone first.</p>
      ) : (
        <form onSubmit={submit} className="flex flex-col gap-3">
          <div className="flex flex-col gap-3 sm:flex-row sm:items-end">
            <label className="flex flex-1 flex-col gap-1.5">
              <span className="label-mono text-text-quiet">Person</span>
              <select className={selectClass} value={person} onChange={(e) => setPerson(e.target.value)}>
                <option value="">Choose a person…</option>
                {people.data!.map((p) => (
                  <option key={p.id} value={p.id}>
                    {p.name} · {p.email}
                  </option>
                ))}
              </select>
            </label>
            <label className="flex flex-col gap-1.5">
              <span className="label-mono text-text-quiet">Access</span>
              <select className={selectClass} value={role} onChange={(e) => setRole(e.target.value as "viewer" | "editor")}>
                <option value="viewer">View only</option>
                <option value="editor">Can edit</option>
              </select>
            </label>
            <Button variant="primary" type="submit" disabled={!person || create.isPending}>Share everything</Button>
          </div>
          <label className="flex min-h-11 cursor-pointer items-center gap-2 text-sm">
            <input type="checkbox" className="accent-[var(--tmp-accent)]" checked={notes} onChange={(e) => setNotes(e.target.checked)} />
            Include my knowledge notes too, not only project notes
          </label>
        </form>
      )}
      <ErrorNote error={create.error ?? update.error ?? remove.error ?? letBackIn.error ?? shares.error} />

      {given.length > 0 && (
        <ShareList title="You share">
          {given.map((s) => (
            <li key={s.id} className="border-rule flex min-h-14 flex-wrap items-center gap-x-3 gap-y-1 border-x-0 border-t-0 py-2 last:border-b-0">
              <Avatar name={s.grantee_name} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate font-bold">{s.grantee_name}</span>
                <span className="label-mono text-text-quiet truncate text-[10px]">{what(s)}</span>
              </span>
              <select
                aria-label={`What ${s.grantee_name} can do`}
                className={selectClass}
                value={s.role}
                onChange={(e) => update.mutate({ id: s.id, role: e.target.value })}
              >
                <option value="viewer">View only</option>
                <option value="editor">Can edit</option>
              </select>
              <button
                type="button"
                className="label-mono text-late min-h-11 cursor-pointer px-2"
                onClick={() => confirm(`Stop sharing ${what(s).toLowerCase()} with ${s.grantee_name}?`) && remove.mutate(s.id)}
              >
                Stop
              </button>
            </li>
          ))}
        </ShareList>
      )}

      {removed.length > 0 && (
        <ShareList title="Removed from one project">
          {removed.map((x) => (
            <li
              key={`${x.project_id}-${x.grantee_id}`}
              className="border-rule flex min-h-14 flex-wrap items-center gap-x-3 gap-y-1 border-x-0 border-t-0 py-2 last:border-b-0"
            >
              <Avatar name={x.grantee_name} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate font-bold">{x.grantee_name}</span>
                <span className="label-mono text-text-quiet truncate text-[10px]">
                  Not{" "}
                  <Link to={`/projects/${x.project_id}`} className="hover:text-accent">
                    {x.project_title}
                  </Link>{" "}
                  or its spawn
                </span>
              </span>
              <Button
                type="button"
                disabled={letBackIn.isPending}
                onClick={() => letBackIn.mutate({ projectId: x.project_id, userId: x.grantee_id })}
              >
                Let back in
              </Button>
            </li>
          ))}
        </ShareList>
      )}

      {received.length > 0 && (
        <ShareList title="Shared with you">
          {received.map((s) => (
            <li key={s.id} className="border-rule flex min-h-14 flex-wrap items-center gap-x-3 gap-y-1 border-x-0 border-t-0 py-2 last:border-b-0">
              <Avatar name={s.owner_name} />
              <span className="flex min-w-0 flex-1 flex-col">
                <span className="truncate font-bold">
                  {s.kind === "project" && s.project_id ? (
                    <Link to={`/projects/${s.project_id}`} className="hover:text-accent">{s.project_title}</Link>
                  ) : (
                    `Everything of ${s.owner_name}’s`
                  )}
                </span>
                <span className="label-mono text-text-quiet truncate text-[10px]">
                  From {s.owner_name} · {s.role === "editor" ? "you can edit" : "view only"}
                </span>
              </span>
              <button
                type="button"
                className="label-mono text-text-quiet hover:text-late min-h-11 cursor-pointer px-2"
                onClick={() => confirm("Leave? You'll stop seeing it until it's shared again.") && remove.mutate(s.id)}
              >
                Leave
              </button>
            </li>
          ))}
        </ShareList>
      )}
    </Panel>
  );
}

function what(s: Share): string {
  if (s.kind === "account") return s.include_notes ? "Everything, with knowledge notes" : "Everything";
  return s.include_children ? `${s.project_title} and its spawn` : (s.project_title ?? "A project");
}

function ShareList({ title, children }: { title: string; children: React.ReactNode }) {
  return (
    <section className="mt-5 flex flex-col gap-1">
      <h3 className="label-mono text-text-quiet">{title}</h3>
      <ul className="flex flex-col">{children}</ul>
    </section>
  );
}

function Avatar({ name }: { name: string }) {
  return (
    <span className="bg-surface-raised label-mono grid size-9 shrink-0 place-items-center rounded-full text-[11px] font-semibold">
      {initials(name)}
    </span>
  );
}
