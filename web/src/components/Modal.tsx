import { useEffect, useRef, type ReactNode } from "react";

/** A centred dialog. Escape or the backdrop closes it; focus goes to the first field. */
export function Modal({ title, onClose, children }: { title: string; onClose: () => void; children: ReactNode }) {
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const previous = document.activeElement as HTMLElement | null;
    ref.current?.querySelector<HTMLElement>("input, textarea, select, button")?.focus();
    const onKey = (e: KeyboardEvent) => e.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => {
      window.removeEventListener("keydown", onKey);
      previous?.focus?.();
    };
  }, [onClose]);

  return (
    <div className="fixed inset-0 z-50 flex items-end justify-center p-0 sm:items-center sm:p-4">
      <div className="bg-shadow/60 absolute inset-0" onClick={onClose} aria-hidden="true" />
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label={title}
        className="border-rule bg-surface-card shadow-card relative max-h-[90vh] w-full max-w-[480px] overflow-y-auto rounded-t-lg border-t-[6px] border-t-accent p-5 sm:rounded-lg"
      >
        <h2 className="font-display mb-4 text-[22px] font-extrabold tracking-[-0.02em]">{title}</h2>
        {children}
      </div>
    </div>
  );
}
