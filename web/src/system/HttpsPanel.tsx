import { useState, type ReactNode } from "react";
import { Button, ErrorNote, Field, Panel } from "../components/ui";
import { covers, httpsOrigin, useAddHttpsHost, useHttpsInfo, useRemoveHttpsHost } from "./queries";

type Os = "mac" | "windows" | "linux";

function guessOs(): Os {
  const ua = navigator.userAgent;
  if (/Windows/i.test(ua)) return "windows";
  if (/Mac|iPhone|iPad/i.test(ua)) return "mac";
  return "linux";
}

/** The line that makes Claude Code (Node) trust the downloaded certificate. */
export function trustCommand(os: Os): string {
  if (os === "windows") return 'setx NODE_EXTRA_CA_CERTS "%USERPROFILE%\\Downloads\\TooManyProjects-CA.pem"';
  const profile = os === "mac" ? "~/.zshrc" : "~/.bashrc";
  return `echo 'export NODE_EXTRA_CA_CERTS="$HOME/Downloads/TooManyProjects-CA.pem"' >> ${profile}`;
}

/**
 * Settings → HTTPS. The server serves HTTPS itself with a certificate it
 * made; each computer trusts it once, then Claude Code connects over the
 * local network with no proxy or tunnel.
 */
export function HttpsPanel({ owner }: { owner: boolean }) {
  const info = useHttpsInfo();
  const add = useAddHttpsHost();
  const remove = useRemoveHttpsHost();
  const [os, setOs] = useState<Os>(guessOs);
  const [copied, setCopied] = useState<string | null>(null);
  const [newHost, setNewHost] = useState("");
  const data = info.data;
  if (!data) return info.isError ? <Panel title="HTTPS"><ErrorNote error={info.error} /></Panel> : null;

  const origin = httpsOrigin(window.location, data);
  const here = window.location.hostname;
  const copy = async (key: string, text: string) => {
    await navigator.clipboard?.writeText(text);
    setCopied(key);
  };
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
  const step = (n: number, title: string, body: ReactNode) => (
    <div className="grid grid-cols-[28px_1fr] items-start gap-3">
      <span className="border-accent text-accent grid size-7 place-items-center rounded-sm border-[1.5px] font-mono text-xs font-semibold">{n}</span>
      <div className="flex min-w-0 flex-col gap-2">
        <span className="font-bold">{title}</span>
        {body}
      </div>
    </div>
  );

  if (data.mode === "off") {
    return (
      <Panel title="HTTPS">
        <p className="text-text-quiet text-sm">
          The server's own HTTPS is off (<code className="font-mono">TMP_HTTPS=off</code>), so it's plain HTTP unless something in front of it adds
          HTTPS.
        </p>
      </Panel>
    );
  }

  if (data.mode === "own") {
    return (
      <Panel title="HTTPS">
        <p className="text-text-quiet text-sm">
          The server uses your own certificate (<code className="font-mono">TMP_TLS_CERT</code>) on port {data.port}
          {origin && (
            <>
              {" "}
              at <code className="text-text font-mono">{origin}</code>
            </>
          )}
          . There's nothing to install.
        </p>
      </Panel>
    );
  }

  const herePort = origin ? new URL(origin).port || "443" : String(data.port);
  return (
    <Panel title="HTTPS">
      <p className="text-text-quiet mb-5 text-sm leading-relaxed">
        The server speaks HTTPS on port <b className="text-text">{data.port}</b> with a certificate it made itself, so Claude Code can sign in
        over your home network with no tunnel. Each computer trusts the certificate once; after that it renews itself.
      </p>

      {!covers(data, here) && (
        <div role="note" className="border-late mb-5 flex flex-col gap-2 rounded-md border-[1.5px] p-3 text-sm leading-relaxed">
          <b className="text-late">The certificate doesn't cover {here} yet</b>
          {owner ? (
            <>
              <span className="text-text-quiet">Add it so connections to this address are trusted.</span>
              <Button type="button" variant="primary" className="self-start" disabled={add.isPending} onClick={() => add.mutate(here)}>
                Add {here}
              </Button>
            </>
          ) : (
            <span className="text-text-quiet">Ask the owner to add it in Settings → HTTPS.</span>
          )}
        </div>
      )}

      <section aria-labelledby="https-steps" className="border-rule bg-surface flex flex-col gap-4 rounded-md border-t-[6px] border-t-accent p-4">
        <h3 id="https-steps" className="font-display text-lg font-extrabold tracking-[-0.02em]">
          Use it from Claude Code
        </h3>
        {step(
          1,
          "Download the certificate",
          <>
            <a
              href="/api/v1/https/certificate"
              download
              className="border-rule hover:bg-surface-raised inline-flex min-h-11 items-center self-start rounded-md px-4 font-bold"
            >
              Download TooManyProjects-CA.pem
            </a>
            {data.fingerprint && (
              <p className="text-text-quiet text-xs break-all">
                SHA-256 <span className="font-mono">{data.fingerprint}</span>
              </p>
            )}
          </>,
        )}
        {step(
          2,
          "Tell Claude Code to trust it",
          <>
            <div role="tablist" aria-label="Computer" className="border-rule flex self-start overflow-hidden rounded-md">
              {(
                [
                  ["mac", "Mac"],
                  ["windows", "Windows"],
                  ["linux", "Linux"],
                ] as const
              ).map(([key, label]) => (
                <button
                  key={key}
                  type="button"
                  role="tab"
                  aria-selected={os === key}
                  onClick={() => setOs(key)}
                  className={`label-mono min-h-10 cursor-pointer px-3.5 ${os === key ? "bg-accent text-on-accent font-semibold" : "text-text-quiet"}`}
                >
                  {label}
                </button>
              ))}
            </div>
            {copyRow("trust", trustCommand(os))}
            <p className="text-text-quiet text-sm leading-relaxed">
              Run it once in a terminal{os === "windows" ? "" : ", or change the path if you saved the file elsewhere"}. Then open a new terminal
              so it takes effect.
            </p>
          </>,
        )}
        {step(
          3,
          "Add the server",
          <>
            {origin && copyRow("add", `claude mcp add --transport http toomanyprojects ${origin}/mcp`)}
            <p className="text-text-quiet text-sm leading-relaxed">
              Then type <code className="text-text font-mono text-[12px]">/mcp</code> in Claude Code, pick toomanyprojects and choose{" "}
              <b className="text-text">Authenticate</b>.
              {herePort !== String(data.port) || window.location.protocol === "https:"
                ? ""
                : ` If Docker maps port ${data.port} to a different port, use that one.`}
            </p>
          </>,
        )}
        <p className="text-text-quiet border-rule border-x-0 border-b-0 pt-3 text-xs leading-relaxed">
          The sign-in page opens in your browser, which warns about the certificate until you trust it there too: open the downloaded file and
          mark it trusted (Mac: Keychain Access, <i>Always Trust</i>; Windows: <i>Install Certificate</i> → <i>Trusted Root Certification
          Authorities</i>). Or continue past the warning; Claude Code still connects.
        </p>
      </section>

      {owner && (
        <section aria-labelledby="https-hosts" className="mt-6 flex flex-col gap-3">
          <h3 id="https-hosts" className="font-display font-extrabold">
            Addresses it covers
          </h3>
          <ul className="flex flex-wrap gap-2">
            {data.hosts.map((h) => (
              <li key={h.host} className="border-rule bg-surface flex min-h-9 items-center gap-2 rounded-md px-3 font-mono text-[13px]">
                {h.host}
                {h.added && (
                  <button
                    type="button"
                    aria-label={`Remove ${h.host}`}
                    className="text-text-quiet hover:text-late cursor-pointer"
                    onClick={() => remove.mutate(h.host)}
                  >
                    ×
                  </button>
                )}
              </li>
            ))}
          </ul>
          <form
            className="flex items-end gap-2"
            onSubmit={(e) => {
              e.preventDefault();
              if (newHost.trim()) add.mutate(newHost.trim(), { onSuccess: () => setNewHost("") });
            }}
          >
            <div className="min-w-0 flex-1">
              <Field label="Add an address" value={newHost} onChange={(e) => setNewHost(e.target.value)} placeholder="nas.local or 192.168.1.50" />
            </div>
            <Button type="submit" disabled={!newHost.trim() || add.isPending}>
              Add
            </Button>
          </form>
          <ErrorNote error={add.error ?? remove.error} />
          <p className="text-text-quiet text-xs leading-relaxed">
            The server finds its own addresses, but inside Docker it can't see your network's, so add the name or IP you open it at. Computers
            that already trust the certificate don't need to do anything again.
          </p>
        </section>
      )}
    </Panel>
  );
}
