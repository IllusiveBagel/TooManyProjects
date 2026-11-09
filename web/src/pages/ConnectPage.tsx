import { useMutation, useQuery } from "@tanstack/react-query";
import { useState } from "react";
import { useLocation, useNavigate } from "react-router";
import { api, unwrap } from "../api/client";
import { useLogout, useMe } from "../auth/queries";
import { AuthLayout } from "../components/AuthLayout";
import { Button, ErrorNote } from "../components/ui";

type Scope = "read" | "write";

const OPTIONS: { scope: Scope; title: string; detail: string }[] = [
  { scope: "read", title: "Read only", detail: "See projects, tasks, notes, data and your calendar." },
  {
    scope: "write",
    title: "Read and write",
    detail: "Also add and move tasks, write notes, log comments, plan time and start spawned projects. Shown as via AI in the history.",
  },
];

/**
 * The Allow page an app such as Claude sends you to when it connects to the
 * MCP server (`/oauth/authorize` lands here). Signing in, if needed, happens first.
 */
export function ConnectPage() {
  const me = useMe();
  const location = useLocation();
  const navigate = useNavigate();
  const logout = useLogout();
  const params = new URLSearchParams(location.search);
  const query = {
    client_id: params.get("client_id") ?? "",
    redirect_uri: params.get("redirect_uri") ?? "",
    response_type: params.get("response_type") ?? undefined,
    code_challenge: params.get("code_challenge") ?? undefined,
    code_challenge_method: params.get("code_challenge_method") ?? undefined,
    scope: params.get("scope") ?? undefined,
  };
  const request = useQuery({
    queryKey: ["oauth-request", location.search],
    queryFn: () => unwrap(api.GET("/oauth/request", { params: { query } })),
    retry: false,
  });
  const [picked, setPicked] = useState<Scope | null>(null);
  const scope = picked ?? (request.data?.suggested_scope as Scope | undefined) ?? "write";
  const decide = useMutation({
    mutationFn: (allow: boolean) =>
      unwrap(
        api.POST("/oauth/approve", {
          body: { ...query, state: params.get("state") ?? undefined, scope, allow },
        }),
      ),
    onSuccess: (data) => window.location.assign(data.redirect_to),
  });

  if (request.isPending) return null;
  if (request.isError)
    return (
      <AuthLayout title="Can't connect this app" intro={request.error.message}>
        <Button onClick={() => navigate("/")}>Go to your projects</Button>
      </AuthLayout>
    );

  const app = request.data;
  const switchAccount = () =>
    logout.mutate(undefined, {
      onSuccess: () => navigate("/login", { replace: true, state: { from: location.pathname + location.search } }),
    });

  return (
    <AuthLayout title={`Let ${app.client_name} use your projects?`}>
      <div className="flex flex-col gap-5">
        <p className="text-text-quiet -mt-4 text-sm">
          Signed in as <b className="text-text">{me.name}</b> ·{" "}
          <button type="button" className="cursor-pointer underline" onClick={switchAccount}>
            not you?
          </button>
        </p>

        <div role="radiogroup" aria-label={`What ${app.client_name} may do`} className="flex flex-col gap-2.5">
          {OPTIONS.map((o) => {
            const on = scope === o.scope;
            return (
              <button
                key={o.scope}
                type="button"
                role="radio"
                aria-checked={on}
                onClick={() => setPicked(o.scope)}
                className={`grid cursor-pointer grid-cols-[22px_1fr] items-start gap-3 rounded-md border-[1.5px] p-3.5 text-left ${
                  on ? "border-accent shadow-[3px_3px_0_var(--tmp-accent)]" : "border-line"
                }`}
              >
                <span
                  aria-hidden
                  className={`mt-0.5 grid size-[18px] place-items-center rounded-full border-[1.5px] ${on ? "border-accent" : "border-text-quiet"}`}
                >
                  {on && <span className="bg-accent size-[9px] rounded-full" />}
                </span>
                <span className="flex flex-col gap-0.5">
                  <b>{o.title}</b>
                  <span className="text-text-quiet text-[13px] leading-snug">{o.detail}</span>
                </span>
              </button>
            );
          })}
        </div>

        <div className={`flex flex-col gap-1 rounded-md border-[1.5px] px-3 py-2.5 ${app.warning ? "border-late" : "border-line"}`}>
          <span className="label-mono text-text-quiet text-[10px]">You'll go back to</span>
          <span className={`font-mono text-[13px] ${app.warning ? "text-late" : ""}`}>{app.redirect_host}</span>
          {app.warning && <span className="text-late text-[13px] leading-snug">{app.warning}</span>}
        </div>

        <p className="text-text-quiet text-[13px] leading-snug">
          {app.client_name} sees what you can see, as you. Disconnect any time in Settings → AI access.
        </p>
        <ErrorNote error={decide.error} />
        <div className="flex justify-end gap-2.5">
          <Button type="button" disabled={decide.isPending} onClick={() => decide.mutate(false)}>
            Cancel
          </Button>
          <Button variant="primary" type="button" disabled={decide.isPending} onClick={() => decide.mutate(true)}>
            Allow
          </Button>
        </div>
      </div>
    </AuthLayout>
  );
}
