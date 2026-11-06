import { useId, type ButtonHTMLAttributes, type InputHTMLAttributes, type ReactNode } from "react";

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
  ...props
}: InputHTMLAttributes<HTMLInputElement> & { label: string; hint?: ReactNode }) {
  const id = useId();
  return (
    <div className="flex flex-col gap-1.5">
      <label htmlFor={id} className="label-mono text-text-quiet">
        {label}
      </label>
      <input
        id={id}
        className="border-rule bg-surface text-text min-h-11 rounded-md px-3 font-mono text-[13px] outline-none focus:border-accent"
        {...props}
      />
      {hint && <p className="text-text-quiet text-xs">{hint}</p>}
    </div>
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
