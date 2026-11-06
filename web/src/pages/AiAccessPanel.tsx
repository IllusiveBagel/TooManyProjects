import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent } from "react";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";
import { Button, ErrorNote, Field, Panel } from "../components/ui";
import { ago } from "../notes/format";

type CreatedToken = components["schemas"]["CreatedToken"];

/** Tokens that let an AI assistant read (and maybe change) your projects over MCP. */
export function AiAccessPanel() {
  const qc = useQueryClient();
  const [name, setName] = useState("");
  const [scope, setScope] = useState<"read" | "write">("read");
  const [created, setCreated] = useState<CreatedToken | null>(null);
  const [copied, setCopied] = useState<string | null>(null);
  const tokens = useQuery({ queryKey: ["tokens"], queryFn: () => unwrap(api.GET("/tokens")) });
  const create = useMutation({
    mutationFn: () => unwrap(api.POST("/tokens", { body: { name: name.trim(), scope } })),
    onSuccess: (data) => {
      setCreated(data);
      setCopied(null);
      setName("");
      qc.invalidateQueries({ queryKey: ["tokens"] });
    },
  });
  const revoke = useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/tokens/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["tokens"] }),
  });

  const url = `${window.location.origin}/mcp`;
  const secret = created?.secret ?? "<your token>";
  const claudeCode = `claude mcp add --transport http toomanyprojects ${url} --header "Authorization: Bearer ${secret}"`;
  const json = JSON.stringify(
    { mcpServers: { toomanyprojects: { type: "http", url, headers: { Authorization: `Bearer ${secret}` } } } },
    null,
    2,
  );
  const copy = async (key: string, text: string) => {
    await navigator.clipboard?.writeText(text);
    setCopied(key);
  };
  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (name.trim()) create.mutate();
  };

  const snippet = (key: string, label: string, text: string) => (
    <div className="flex flex-col gap-1.5">
      <span className="label-mono text-text-quiet">{label}</span>
      <div className="flex items-start gap-2">
        <pre className="bg-surface-raised min-w-0 flex-1 overflow-x-auto rounded-sm px-2.5 py-2 font-mono text-[11.5px] leading-relaxed">{text}</pre>
        <Button type="button" className="min-h-9 px-3 text-sm" onClick={() => copy(key, text)}>
          {copied === key ? "Copied" : "Copy"}
        </Button>
      </div>
    </div>
  );

  return (
    <Panel title="AI access">
      <p className="text-text-quiet mb-4 text-sm leading-relaxed">
        Let an AI assistant such as Claude read your projects, tasks and notes through the MCP server at{" "}
        <code className="font-mono text-[12px]">{url}</code>. Each token acts as you. A read-only token can look; a read-write token can
        also add and move tasks, write notes and start spawned projects, and those changes show as <em>via AI</em> in the history.
      </p>
      <form onSubmit={submit} className="flex flex-col gap-3 sm:flex-row sm:items-end">
        <div className="flex-1">
          <Field label="Name" placeholder="Claude on my laptop" value={name} maxLength={80} onChange={(e) => setName(e.target.value)} />
        </div>
        <label className="flex flex-col gap-1.5">
          <span className="label-mono text-text-quiet">Access</span>
          <select
            value={scope}
            onChange={(e) => setScope(e.target.value as "read" | "write")}
            className="border-rule bg-surface min-h-11 rounded-md px-2 font-mono text-[13px] outline-none focus:border-accent"
          >
            <option value="read">Read only</option>
            <option value="write">Read and write</option>
          </select>
        </label>
        <Button variant="primary" type="submit" disabled={create.isPending || !name.trim()}>Make token</Button>
      </form>
      <ErrorNote error={create.error ?? revoke.error} />

      {created && (
        <div className="border-rule bg-surface mt-4 flex flex-col gap-3 rounded-md border-accent p-3">
          <span className="label-mono text-accent">Copy it now, it won't be shown again</span>
          {snippet("secret", "Token", created.secret)}
          {snippet("claude", "Claude Code", claudeCode)}
          {snippet("json", "Other MCP clients (JSON config)", json)}
        </div>
      )}

      {(tokens.data?.length ?? 0) > 0 && (
        <ul className="mt-5 flex flex-col" aria-label="Tokens">
          {tokens.data!.map((t) => (
            <li key={t.id} className="border-rule flex min-h-12 flex-wrap items-center gap-x-3 gap-y-1 border-x-0 border-t-0 py-2 last:border-b-0">
              <span className="min-w-0 basis-full truncate sm:basis-0 sm:flex-1">{t.name}</span>
              <span className={`label-mono ${t.scope === "write" ? "text-accent" : "text-text-quiet"}`}>
                {t.scope === "write" ? "Read and write" : "Read only"}
              </span>
              <span className="label-mono text-text-quiet text-[10px]">
                {t.revoked_at ? "Revoked" : t.last_used_at ? `Used ${ago(t.last_used_at)}` : "Never used"}
              </span>
              {!t.revoked_at && (
                <button
                  type="button"
                  className="label-mono text-late min-h-11 cursor-pointer px-2"
                  onClick={() => confirm(`Revoke “${t.name}”? Anything using it stops working.`) && revoke.mutate(t.id)}
                >
                  Revoke
                </button>
              )}
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}
