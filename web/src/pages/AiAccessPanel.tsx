import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useState, type FormEvent, type ReactNode } from "react";
import { api, unwrap } from "../api/client";
import type { components } from "../api/schema";
import { httpsOrigin, useHttpsInfo } from "../system/queries";
import { Button, ErrorNote, Field, Panel } from "../components/ui";
import { ago } from "../notes/format";

type CreatedToken = components["schemas"]["CreatedToken"];

/** API tokens, for scripts and apps that can't sign in. */
function TokensSection() {
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
    <div className="flex flex-col">
      <p className="text-text-quiet mb-4 text-sm leading-relaxed">
        For scripts and apps that can't sign in. Send a token as <code className="font-mono text-[12px]">Authorization: Bearer</code>{" "}
        to <code className="font-mono text-[12px]">{url}</code>. Each token acts as you; a read-only token can look, a read-write token can
        also change things.
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
    </div>
  );
}

const HTTPS_GUIDE = "https://github.com/IllusiveBagel/TooManyProjects/blob/main/docs/guide.md#put-it-online-with-https";

/** True when the Claude app (which connects from the internet) can't reach this address. */
export function unreachableFromInternet(origin: URL): boolean {
  if (origin.protocol !== "https:") return true;
  const host = origin.hostname;
  return (
    host === "localhost" ||
    host.endsWith(".local") ||
    host.endsWith(".lan") ||
    host.endsWith(".home") ||
    host.endsWith(".internal") ||
    !host.includes(".") ||
    /^(127\.|10\.|192\.168\.|172\.(1[6-9]|2\d|3[01])\.|169\.254\.)/.test(host) ||
    host.startsWith("[")
  );
}

/** Connect Claude by signing in, the apps you've connected, and API tokens. */
export function AiAccessPanel() {
  const qc = useQueryClient();
  const [which, setWhich] = useState<"app" | "code">("app");
  const [copied, setCopied] = useState<string | null>(null);
  const connections = useQuery({ queryKey: ["oauth-connections"], queryFn: () => unwrap(api.GET("/oauth/connections")) });
  const disconnect = useMutation({
    mutationFn: (id: string) => unwrap(api.DELETE("/oauth/connections/{id}", { params: { path: { id } } })),
    onSuccess: () => qc.invalidateQueries({ queryKey: ["oauth-connections"] }),
  });
  const tokens = useQuery({ queryKey: ["tokens"], queryFn: () => unwrap(api.GET("/tokens")) });
  const liveTokens = tokens.data?.filter((t) => !t.revoked_at).length ?? 0;

  const https = useHttpsInfo();
  // Claude Code needs https to sign in; use the server's own HTTPS when this page isn't.
  const base = httpsOrigin(window.location, https.data) ?? window.location.origin;
  const url = `${base}/mcp`;
  const offline = unreachableFromInternet(new URL(base));
  const ownCert = https.data?.mode === "generated" && window.location.protocol !== "https:";
  const claudeCode = `claude mcp add --transport http toomanyprojects ${url}`;
  const copy = async (key: string, text: string) => {
    await navigator.clipboard?.writeText(text);
    setCopied(key);
  };
  const step = (n: number, title: string, body: ReactNode) => (
    <div className="grid grid-cols-[28px_1fr] items-start gap-3">
      <span className="border-accent text-accent grid size-7 place-items-center rounded-sm border-[1.5px] font-mono text-xs font-semibold">{n}</span>
      <div className="flex min-w-0 flex-col gap-2">
        <span className="font-bold">{title}</span>
        {body}
      </div>
    </div>
  );
  const copyRow = (key: string, text: string) => (
    <div className="flex items-stretch gap-2">
      <code className="border-rule bg-surface flex min-h-11 min-w-0 flex-1 items-center overflow-x-auto rounded-md px-3 font-mono text-[13px] whitespace-nowrap">
        {text}
      </code>
      <Button variant="primary" type="button" className="min-w-[92px]" onClick={() => copy(key, text)}>
        {copied === key ? "Copied" : "Copy"}
      </Button>
    </div>
  );

  return (
    <Panel title="AI access">
      <p className="text-text-quiet mb-5 text-sm leading-relaxed">
        Let Claude read your projects, tasks and notes, and (if you allow it) add and move things for you. Anything it changes shows as{" "}
        <em>via AI</em> in the history.
      </p>

      <section aria-labelledby="connect-claude" className="border-rule bg-surface flex flex-col gap-4 rounded-md border-t-[6px] border-t-accent p-4">
        <h3 id="connect-claude" className="font-display text-lg font-extrabold tracking-[-0.02em]">
          Connect Claude
        </h3>
        {offline && (
          <div role="note" className="border-late flex flex-col gap-2 rounded-md border-[1.5px] p-3 text-sm leading-relaxed">
            <b className="text-late">The Claude app can't reach this address</b>
            <span className="text-text-quiet">
              You're using <code className="text-text font-mono text-[12px]">{window.location.origin}</code>. Claude on the web, desktop and phone
              connects from the internet, so it needs an <b className="text-text">https://</b> address it can reach, set as{" "}
              <code className="text-text font-mono text-[12px]">TMP_PUBLIC_URL</code>. Claude Code on this network still works.
            </span>
            <a href={HTTPS_GUIDE} target="_blank" rel="noreferrer" className="label-mono text-accent">
              How to put it online with HTTPS ›
            </a>
          </div>
        )}
        {step(1, "Copy your server address", copyRow("url", url))}
        {step(
          2,
          "Add it in Claude",
          <>
            <div role="tablist" aria-label="Which Claude" className="border-rule flex self-start overflow-hidden rounded-md">
              {(
                [
                  ["app", "Claude app"],
                  ["code", "Claude Code"],
                ] as const
              ).map(([key, label]) => (
                <button
                  key={key}
                  type="button"
                  role="tab"
                  aria-selected={which === key}
                  onClick={() => setWhich(key)}
                  className={`label-mono min-h-10 cursor-pointer px-3.5 ${which === key ? "bg-accent text-on-accent font-semibold" : "text-text-quiet"}`}
                >
                  {label}
                </button>
              ))}
            </div>
            {which === "app" ? (
              <p className="text-text-quiet text-sm leading-relaxed">
                On claude.ai, the desktop app or your phone: <b className="text-text">Settings → Connectors → Add custom connector</b>. Name it{" "}
                <b className="text-text">TooManyProjects</b>, paste the address, press <b className="text-text">Add</b>, then{" "}
                <b className="text-text">Connect</b>.
              </p>
            ) : (
              <>
                {ownCert && (
                  <p className="text-text-quiet text-sm leading-relaxed">
                    First trust the server's certificate on this computer: see <b className="text-text">HTTPS</b> below.
                  </p>
                )}
                {copyRow("code", claudeCode)}
                <p className="text-text-quiet text-sm leading-relaxed">
                  Then type <code className="text-text font-mono text-[12px]">/mcp</code> in Claude Code, pick toomanyprojects and choose{" "}
                  <b className="text-text">Authenticate</b>.
                </p>
              </>
            )}
          </>,
        )}
        {step(
          3,
          "Sign in and press Allow",
          <p className="text-text-quiet text-sm leading-relaxed">
            A TooManyProjects page opens. Sign in if asked, choose what Claude may do, and you're sent back to Claude, connected.
          </p>,
        )}
      </section>

      <section aria-labelledby="connected-apps" className="mt-6 flex flex-col">
        <h3 id="connected-apps" className="font-display mb-1 font-extrabold">
          Connected apps
        </h3>
        {connections.data?.length === 0 && <p className="text-text-quiet py-2 text-sm">Nothing connected yet.</p>}
        <ul className="flex flex-col" aria-label="Connected apps">
          {connections.data?.map((c) => (
            <li key={c.id} className="border-rule flex min-h-14 flex-wrap items-center gap-x-4 gap-y-1 border-x-0 border-t-0 py-2 last:border-b-0">
              <span className="flex min-w-[200px] flex-1 flex-col">
                <b>{c.client_name}</b>
                <span className="text-text-quiet font-mono text-[11px]">
                  {c.redirect_host} · connected {ago(c.created_at)}
                </span>
              </span>
              <span className={`label-mono ${c.scope === "write" ? "text-accent" : "text-text-quiet"}`}>
                {c.scope === "write" ? "Read and write" : "Read only"}
              </span>
              <span className="label-mono text-text-quiet min-w-[110px] text-[10px]">
                {c.last_used_at ? `Used ${ago(c.last_used_at)}` : "Never used"}
              </span>
              <button
                type="button"
                className="label-mono text-late min-h-11 cursor-pointer px-2"
                onClick={() => confirm(`Disconnect “${c.client_name}”? It stops working at once.`) && disconnect.mutate(c.id)}
              >
                Disconnect
              </button>
            </li>
          ))}
        </ul>
        <ErrorNote error={connections.error ?? disconnect.error} />
        {(connections.data?.length ?? 0) > 0 && (
          <p className="text-text-quiet mt-2 text-[13px]">To change read only to read and write, disconnect and connect again.</p>
        )}
      </section>

      <details className="border-rule group mt-6 rounded-md">
        <summary className="flex min-h-14 cursor-pointer list-none items-center gap-3 px-4">
          <svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" aria-hidden className="transition-transform group-open:rotate-90">
            <path d="M9 6l6 6-6 6" />
          </svg>
          <span className="font-extrabold">Advanced: API tokens</span>
          <span className="text-text-quiet hidden text-[13px] sm:inline">For scripts and apps that can't sign in</span>
          <span className="label-mono text-text-quiet ml-auto text-[10px]">
            {liveTokens} token{liveTokens === 1 ? "" : "s"}
          </span>
        </summary>
        <div className="border-rule border-x-0 border-b-0 p-4">
          <TokensSection />
        </div>
      </details>
    </Panel>
  );
}
