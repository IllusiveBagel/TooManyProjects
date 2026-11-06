import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { api, unwrap, type CreatedInvite } from "../api/client";
import { useMe, useUpdatePreferences } from "../auth/queries";
import { Button, ErrorNote, Field, Panel } from "../components/ui";
import { useThemeStore } from "../theme/store";
import type { ModePreference } from "../theme/types";

export function SettingsPage() {
  const me = useMe();
  return (
    <main className="mx-auto flex w-full max-w-[880px] flex-col gap-6 px-4 py-8">
      <h1 className="font-display text-[32px] font-extrabold tracking-[-0.02em]">Settings</h1>
      <AppearancePanel />
      {me.role === "owner" && <InvitesPanel />}
    </main>
  );
}

function AppearancePanel() {
  const me = useMe();
  const update = useUpdatePreferences();
  const { theme, mode } = useThemeStore();
  const themes = useQuery({ queryKey: ["themes"], queryFn: () => unwrap(api.GET("/themes")) });
  const modes: { value: ModePreference; label: string }[] = [
    { value: "system", label: "Follow system" },
    { value: "dark", label: theme.modes.dark ? `${theme.modes.dark.label} (dark)` : "Dark" },
    { value: "light", label: theme.modes.light ? `${theme.modes.light.label} (light)` : "Light" },
  ];

  return (
    <Panel title="Appearance">
      <div className="flex flex-col gap-5">
        {(themes.data?.length ?? 0) > 1 && (
          <label className="flex flex-col gap-1.5">
            <span className="label-mono text-text-quiet">Theme</span>
            <select
              className="border-rule bg-surface min-h-11 rounded-md px-3"
              value={me.theme_id}
              onChange={(e) => update.mutate({ theme_id: e.target.value })}
            >
              {themes.data?.map((t) => <option key={t.id} value={t.id}>{t.name}</option>)}
            </select>
          </label>
        )}
        <fieldset className="flex flex-col gap-1.5">
          <legend className="label-mono text-text-quiet mb-1.5">Mode</legend>
          <div className="border-rule inline-flex w-fit flex-wrap overflow-hidden rounded-md">
            {modes.map((m) => (
              <label
                key={m.value}
                className={`label-mono flex min-h-11 cursor-pointer items-center px-4 ${me.theme_mode === m.value ? "bg-accent text-on-accent" : "text-text-quiet hover:text-text"}`}
              >
                <input
                  type="radio"
                  name="mode"
                  className="sr-only"
                  checked={me.theme_mode === m.value}
                  onChange={() => update.mutate({ theme_mode: m.value })}
                />
                {m.label}
              </label>
            ))}
          </div>
        </fieldset>
        <ErrorNote error={update.error} />
        <div>
          <p className="label-mono text-text-quiet mb-2">Project colours · showing {mode}</p>
          <div className="grid grid-cols-4 gap-2 sm:grid-cols-8">
            {Array.from({ length: 8 }, (_, i) => (
              <div key={i} className="border-rule bg-surface rounded-md p-2">
                <div className="h-8 rounded-sm" style={{ background: `var(--tmp-slot-${i + 1})` }} />
                <span className="label-mono text-text-quiet mt-1.5 block">Slot {i + 1}</span>
              </div>
            ))}
          </div>
        </div>
      </div>
    </Panel>
  );
}

function InvitesPanel() {
  const qc = useQueryClient();
  const [note, setNote] = useState("");
  const [created, setCreated] = useState<CreatedInvite | null>(null);
  const [copied, setCopied] = useState(false);
  const invites = useQuery({ queryKey: ["invites"], queryFn: () => unwrap(api.GET("/invites")) });
  const create = useMutation({
    mutationFn: () => unwrap(api.POST("/invites", { body: { note: note || null } })),
    onSuccess: (data) => {
      setCreated(data);
      setCopied(false);
      setNote("");
      qc.invalidateQueries({ queryKey: ["invites"] });
    },
  });
  const revoke = useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/invites/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["invites"] }),
  });

  const link = created ? (created.url ?? `${window.location.origin}/invite/${created.token}`) : "";
  const copy = async () => {
    await navigator.clipboard?.writeText(link);
    setCopied(true);
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    create.mutate();
  };

  return (
    <Panel title="Invites">
      <p className="text-text-quiet mb-4 text-sm leading-relaxed">
        People join only through a link you send them. Each link works once and lasts 7 days.
      </p>
      <form onSubmit={submit} className="flex flex-col gap-3 sm:flex-row sm:items-end">
        <div className="flex-1">
          <Field label="Who is it for (optional)" value={note} maxLength={200} onChange={(e) => setNote(e.target.value)} />
        </div>
        <Button variant="primary" type="submit" disabled={create.isPending}>Make invite link</Button>
      </form>
      <ErrorNote error={create.error ?? revoke.error} />

      {created && (
        <div className="border-rule bg-surface mt-4 flex flex-col gap-2 rounded-md border-accent p-3">
          <span className="label-mono text-accent">Copy it now, it won't be shown again</span>
          <div className="flex flex-col gap-2 sm:flex-row">
            <code className="bg-surface-raised flex-1 overflow-x-auto rounded-sm px-2 py-2 font-mono text-xs whitespace-nowrap">{link}</code>
            <Button type="button" onClick={copy}>{copied ? "Copied" : "Copy"}</Button>
          </div>
        </div>
      )}

      {(invites.data?.length ?? 0) > 0 && (
        <ul className="mt-5 flex flex-col">
          {invites.data!.map((inv) => {
            const state = inv.accepted_at
              ? `Joined · ${inv.accepted_by_name ?? ""}`
              : inv.revoked_at
                ? "Cancelled"
                : new Date(inv.expires_at) < new Date()
                  ? "Expired"
                  : `Open · until ${new Date(inv.expires_at).toLocaleDateString()}`;
            const open = state.startsWith("Open");
            return (
              <li key={inv.id} className="border-rule flex min-h-12 items-center gap-3 border-x-0 border-t-0 py-2 last:border-b-0">
                <span className="flex-1 truncate">{inv.note ?? "Invite"}</span>
                <span className={`label-mono ${open ? "text-accent" : "text-text-quiet"}`}>{state}</span>
                {open && (
                  <button type="button" className="label-mono text-late min-h-11 cursor-pointer px-2" onClick={() => revoke.mutate(inv.id)}>
                    Cancel
                  </button>
                )}
              </li>
            );
          })}
        </ul>
      )}
    </Panel>
  );
}
