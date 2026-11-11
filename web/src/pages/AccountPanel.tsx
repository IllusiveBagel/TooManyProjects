import { useState, type FormEvent } from "react";
import { useChangePassword, useEndOtherSessions, useEndSession, useLogout, useMe, useSessions } from "../auth/queries";
import { deviceName } from "../auth/device";
import { Button, ErrorNote, Field, Panel } from "../components/ui";
import { ago } from "../notes/format";

const MIN_PASSWORD = 10;

/** Who you are, your password, and where you're signed in. */
export function AccountPanel() {
  const me = useMe();
  const logout = useLogout();
  return (
    <Panel title="Account">
      <div className="flex flex-col gap-6">
        <div className="flex flex-wrap items-center gap-4">
          <div className="flex-1">
            <p className="font-bold">{me.name}</p>
            <p className="text-text-quiet font-mono text-[13px]">{me.email}</p>
          </div>
          <Button type="button" onClick={() => logout.mutate()} disabled={logout.isPending}>
            Sign out
          </Button>
        </div>
        <PasswordForm />
        <SessionList />
      </div>
    </Panel>
  );
}

function PasswordForm() {
  const [open, setOpen] = useState(false);
  const [current, setCurrent] = useState("");
  const [next, setNext] = useState("");
  const [again, setAgain] = useState("");
  const change = useChangePassword();
  const mismatch = again.length > 0 && next !== again;
  const ready = current.length > 0 && next.length >= MIN_PASSWORD && next === again;

  const submit = (e: FormEvent) => {
    e.preventDefault();
    if (!ready) return;
    change.mutate(
      { current_password: current, new_password: next },
      {
        onSuccess: () => {
          setCurrent("");
          setNext("");
          setAgain("");
          setOpen(false);
        },
      },
    );
  };

  return (
    <div className="border-rule flex flex-col gap-3 border-x-0 border-b-0 pt-4">
      <div className="flex flex-wrap items-center gap-3">
        <div className="min-w-[14rem] flex-1">
          <h3 className="font-display font-extrabold">Password</h3>
          {change.data && !open ? (
            <p role="status" className="text-accent text-sm">
              Password changed.{" "}
              {change.data.ended > 0
                ? `Signed out ${change.data.ended} other ${change.data.ended === 1 ? "device" : "devices"}.`
                : "No other devices were signed in."}
            </p>
          ) : (
            <p className="text-text-quiet text-sm">Changing it signs out every other browser and device. This one stays signed in.</p>
          )}
        </div>
        {!open && (
          <Button type="button" onClick={() => { change.reset(); setOpen(true); }}>
            Change password
          </Button>
        )}
      </div>
      {open && (
        <form onSubmit={submit} className="flex flex-col gap-3">
          <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
            <Field label="Current password" type="password" autoComplete="current-password" value={current}
              onChange={(e) => setCurrent(e.target.value)} autoFocus />
            <Field label="New password" type="password" autoComplete="new-password" value={next}
              onChange={(e) => setNext(e.target.value)}
              hint={next.length > 0 && next.length < MIN_PASSWORD ? `At least ${MIN_PASSWORD} characters.` : undefined} />
            <Field label="New password again" type="password" autoComplete="new-password" value={again}
              onChange={(e) => setAgain(e.target.value)} hint={mismatch ? "Doesn't match." : undefined} />
          </div>
          <ErrorNote error={change.error} />
          <div className="flex flex-wrap justify-end gap-2">
            <Button type="button" onClick={() => setOpen(false)}>Cancel</Button>
            <Button type="submit" variant="primary" disabled={!ready || change.isPending}>
              {change.isPending ? "Saving…" : "Change password"}
            </Button>
          </div>
        </form>
      )}
    </div>
  );
}

function SessionList() {
  const sessions = useSessions();
  const end = useEndSession();
  const endOthers = useEndOtherSessions();
  const list = sessions.data ?? [];
  const others = list.filter((s) => !s.current).length;

  return (
    <div className="border-rule flex flex-col gap-3 border-x-0 border-b-0 pt-4">
      <div className="flex flex-wrap items-center gap-3">
        <div className="min-w-[14rem] flex-1">
          <h3 className="font-display font-extrabold">Signed in on</h3>
          <p className="text-text-quiet text-sm">Sign out anything you don't recognise, or a device you've lost.</p>
        </div>
        {others > 0 && (
          <Button type="button" onClick={() => endOthers.mutate()} disabled={endOthers.isPending}>
            Sign out everywhere else
          </Button>
        )}
      </div>
      <ErrorNote error={sessions.error ?? end.error ?? endOthers.error} />
      <ul className="flex flex-col" data-testid="session-list">
        {list.map((s) => (
          <li key={s.id} className="border-rule flex min-h-12 items-center gap-3 border-x-0 border-t-0 py-2 last:border-b-0">
            <span className="flex min-w-0 flex-1 flex-col">
              <span className="truncate" title={s.user_agent ?? undefined}>{deviceName(s.user_agent)}</span>
              <span className="label-mono text-text-quiet truncate text-[10px]">
                {s.current ? "Active now" : `Last used ${ago(s.last_seen_at)}`} · signed in {ago(s.created_at)}
              </span>
            </span>
            {s.current ? (
              <span className="label-mono text-accent">This device</span>
            ) : (
              <button
                type="button"
                className="label-mono text-late min-h-11 cursor-pointer px-2"
                disabled={end.isPending}
                onClick={() => end.mutate(s.id)}
              >
                Sign out
              </button>
            )}
          </li>
        ))}
      </ul>
    </div>
  );
}
