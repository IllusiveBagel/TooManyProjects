import { useId, useState, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";

type Variant = "primary" | "ghost";

export function Button({
  variant = "ghost",
  className = "",
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & { variant?: Variant }) {
  const look =
    variant === "primary"
      ? "bg-accent text-on-accent border-accent shadow-button hover:-translate-x-px hover:-translate-y-px active:translate-x-0 active:translate-y-0 active:shadow-none"
      : "bg-transparent text-text hover:bg-surface-raised";
  return (
    <button
      className={`border-rule inline-flex min-h-11 cursor-pointer items-center justify-center gap-2 rounded-md px-4 font-bold transition-transform disabled:cursor-not-allowed disabled:opacity-60 ${look} ${className}`}
      {...props}
    />
  );
}

export function Field({
  label,
  hint,
  type,
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: ReactNode }) {
  const id = useId();
  const [shown, setShown] = useState(false);
  const isPassword = type === "password";
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="label-mono text-text-quiet">
        {label}
      </label>
      <div className="relative">
        <input
          id={id}
          type={isPassword && shown ? "text" : type}
          className={`border-rule bg-surface text-text min-h-11 w-full rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent ${isPassword ? "pr-12" : ""}`}
          {...props}
        />
        {isPassword && (
          <button
            type="button"
            aria-label={shown ? "Hide password" : "Show password"}
            aria-pressed={shown}
            title={shown ? "Hide password" : "Show password"}
            onClick={() => setShown((v) => !v)}
            className="text-text-quiet hover:text-text absolute inset-y-0 right-0 grid w-11 cursor-pointer place-items-center"
          >
            <EyeIcon open={!shown} />
          </button>
        )}
      </div>
      {hint && <p className="text-text-quiet text-xs">{hint}</p>}
    </div>
  );
}

function EyeIcon({ open }: { open: boolean }) {
  return (
    <svg width="20" height="20" viewBox="0 0 20 20" fill="none" aria-hidden="true">
      <path d="M1.5 10S4.5 4 10 4s8.5 6 8.5 6-3 6-8.5 6S1.5 10 1.5 10Z" stroke="currentColor" strokeWidth="1.6" strokeLinejoin="round" />
      <circle cx="10" cy="10" r="2.6" stroke="currentColor" strokeWidth="1.6" />
      {!open && <path d="M3 17 17 3" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" />}
    </svg>
  );
}

export function Wordmark({ count }: { count?: number }) {
  return (
    <span className="font-display flex items-center gap-2 text-xl font-extrabold tracking-[-0.02em]">
      too many projects
      {count !== undefined && (
        <span className="label-mono bg-accent text-on-accent rounded-sm px-1.5 py-0.5 font-semibold">{count}</span>
      )}
    </span>
  );
}

export function ErrorNote({ error }: { error: unknown }) {
  if (!error) return null;
  const message = error instanceof Error ? error.message : "Something went wrong.";
  return (
    <p role="alert" className="border-rule text-late rounded-md border-current px-3 py-2 text-sm">
      {message}
    </p>
  );
}

export function Panel({ title, children, className = "" }: { title?: string; children: ReactNode; className?: string }) {
  return (
    <section className={`border-rule bg-surface-card shadow-card rounded-lg p-5 ${className}`}>
      {title && <h2 className="font-display mb-4 text-lg font-extrabold tracking-[-0.02em]">{title}</h2>}
      {children}
    </section>
  );
}

/** Marks a change made by an AI assistant through the MCP server. */
export function ViaBadge({ via }: { via?: string }) {
  if (via !== "ai") return null;
  return (
    <span className="label-mono border-rule text-accent ml-1.5 inline-block rounded-sm border-current px-1 py-px align-[1px] text-[9.5px]">
      via AI
    </span>
  );
}
