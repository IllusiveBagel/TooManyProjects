import { useMutation } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { Link } from "react-router";
import { api, unwrap, type CreatedInvite, type Project } from "../api/client";
import { useMe } from "../auth/queries";
import { Modal } from "../components/Modal";
import { Button, ErrorNote } from "../components/ui";
import { initials, ROLE_LABEL } from "./format";
import { useCreateShare, useDeleteShare, useMembers, usePeople, useUpdateShare, type Member } from "./queries";

const selectClass =
  "border-rule bg-surface min-h-11 rounded-md px-2 font-mono text-[13px] outline-none focus:border-accent";

const VIA: Record<string, string> = {
  owner: "Owner",
  project: "Shared this project",
  account: "Shared everything",
  family: "Through a parent project",
};

/** Who can see a project. Its owner can add people, change roles and remove them. */
export function ShareDialog({ project, onClose }: { project: Project; onClose: () => void }) {
  const me = useMe();
  const members = useMembers(project.id);
  const people = usePeople();
  const create = useCreateShare();
  const update = useUpdateShare();
  const remove = useDeleteShare();
  const isOwner = project.role === "owner";
  const [person, setPerson] = useState("");
  const [role, setRole] = useState<"viewer" | "editor">("editor");
  const [children, setChildren] = useState(true);

  const directly = new Set((members.data ?? []).filter((m) => m.via === "project" || m.via === "owner").map((m) => m.user_id));
  const candidates = (people.data ?? []).filter((p) => !directly.has(p.id));

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!person) return;
    create.mutate(
      { grantee_id: person, project_id: project.id, role, include_children: children },
      { onSuccess: () => setPerson("") },
    );
  };

  return (
    <Modal title={`Share ${project.title}`} onClose={onClose}>
      <div className="flex flex-col gap-5">
        <ul aria-label="People with access" className="flex flex-col">
          {(members.data ?? []).map((m) => (
            <MemberRow
              key={m.user_id}
              member={m}
              editable={isOwner && m.via === "project"}
              onRole={(r) => m.share_id && update.mutate({ id: m.share_id, role: r })}
              onRemove={() => m.share_id && remove.mutate(m.share_id)}
            />
          ))}
        </ul>

        {isOwner ? (
          <form onSubmit={submit} className="border-rule flex flex-col gap-3 border-x-0 border-b-0 pt-4">
            <span className="label-mono text-text-quiet">Add someone</span>
            {candidates.length === 0 ? (
              <p className="text-text-quiet text-sm">Everyone with an account here already has it.</p>
            ) : (
              <>
                <div className="grid grid-cols-[1fr_auto] gap-2">
                  <select aria-label="Person" className={selectClass} value={person} onChange={(e) => setPerson(e.target.value)}>
                    <option value="">Choose a person…</option>
                    {candidates.map((p) => (
                      <option key={p.id} value={p.id}>
                        {p.name} · {p.email}
                      </option>
                    ))}
                  </select>
                  <select aria-label="Role" className={selectClass} value={role} onChange={(e) => setRole(e.target.value as "viewer" | "editor")}>
                    <option value="editor">Can edit</option>
                    <option value="viewer">View only</option>
                  </select>
                </div>
                <label className="flex min-h-11 cursor-pointer items-center gap-2 text-sm">
                  <input type="checkbox" className="accent-[var(--tmp-accent)]" checked={children} onChange={(e) => setChildren(e.target.checked)} />
                  Include projects spawned from it
                </label>
                <Button variant="primary" type="submit" disabled={!person || create.isPending}>Share</Button>
              </>
            )}
            <ErrorNote error={create.error ?? update.error ?? remove.error} />
            {me.role === "owner" && <InviteToProject project={project} />}
            <p className="text-text-quiet text-xs leading-relaxed">
              Viewers can see the board, tasks and the project's notes. Editors can also change them. Only you can delete
              the project or change who has it. To share everything you have, use{" "}
              <Link to="/settings" className="text-accent underline">Settings → Sharing</Link>.
            </p>
          </form>
        ) : (
          <p className="text-text-quiet text-sm">Only {project.owner_name} can change who has this project.</p>
        )}
      </div>
    </Modal>
  );
}

function MemberRow({
  member,
  editable,
  onRole,
  onRemove,
}: {
  member: Member;
  editable: boolean;
  onRole: (role: "viewer" | "editor") => void;
  onRemove: () => void;
}) {
  return (
    <li className="border-rule flex min-h-14 items-center gap-3 border-x-0 border-t-0 py-2 last:border-b-0">
      <span className="bg-surface-raised label-mono grid size-9 shrink-0 place-items-center rounded-full text-[11px] font-semibold">
        {initials(member.name)}
      </span>
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="truncate font-bold">{member.name}</span>
        <span className="label-mono text-text-quiet truncate text-[10px]">{VIA[member.via] ?? member.via}</span>
      </span>
      {editable ? (
        <>
          <select
            aria-label={`${member.name}'s role`}
            className={selectClass}
            value={member.role}
            onChange={(e) => onRole(e.target.value as "viewer" | "editor")}
          >
            <option value="editor">Can edit</option>
            <option value="viewer">View only</option>
          </select>
          <button
            type="button"
            aria-label={`Stop sharing with ${member.name}`}
            className="text-text-quiet hover:text-late grid size-11 cursor-pointer place-items-center text-xl"
            onClick={onRemove}
          >
            ×
          </button>
        </>
      ) : (
        <span className={`label-mono text-[11px] ${member.role === "viewer" ? "text-text-quiet" : "text-accent"}`}>
          {ROLE_LABEL[member.role] ?? member.role}
        </span>
      )}
    </li>
  );
}

/** For the install's owner: a link that makes an account and shares this project in one go. */
function InviteToProject({ project }: { project: Project }) {
  const [role, setRole] = useState<"viewer" | "editor">("editor");
  const [created, setCreated] = useState<CreatedInvite | null>(null);
  const [copied, setCopied] = useState(false);
  const make = useMutation({
    mutationFn: () =>
      unwrap(api.POST("/invites", { body: { scope: "project", project_id: project.id, role, include_children: true } })),
    onSuccess: (data) => {
      setCreated(data);
      setCopied(false);
    },
  });
  const link = created ? (created.url ?? `${window.location.origin}/invite/${created.token}`) : "";
  return (
    <div className="flex flex-col gap-2">
      <span className="label-mono text-text-quiet">Or invite someone new</span>
      <div className="grid grid-cols-[1fr_auto] gap-2">
        <select aria-label="Invite role" className={selectClass} value={role} onChange={(e) => setRole(e.target.value as "viewer" | "editor")}>
          <option value="editor">They can edit</option>
          <option value="viewer">They can view</option>
        </select>
        <Button type="button" disabled={make.isPending} onClick={() => make.mutate()}>Make link</Button>
      </div>
      <ErrorNote error={make.error} />
      {created && (
        <div className="border-rule bg-surface flex flex-col gap-2 rounded-md border-accent p-3">
          <span className="label-mono text-accent">Copy it now, it won't be shown again</span>
          <div className="flex flex-col gap-2 sm:flex-row">
            <code className="bg-surface-raised flex-1 overflow-x-auto rounded-sm px-2 py-2 font-mono text-xs whitespace-nowrap">{link}</code>
            <Button
              type="button"
              onClick={async () => {
                await navigator.clipboard?.writeText(link);
                setCopied(true);
              }}
            >
              {copied ? "Copied" : "Copy"}
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
