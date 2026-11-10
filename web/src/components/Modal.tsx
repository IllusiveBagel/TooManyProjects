import { useEffect, useRef, type ReactNode } from "react";

/** A centred dialog. Escape or the backdrop closes it; focus goes to the first field.
 *  `wide` makes room for a grid of choices; `actions` sit beside the title. */
export function Modal({
  title,
  onClose,
  children,
  wide = false,
  actions,
}: {
  title: string;
  onClose: () => void;
  children: ReactNode;
  wide?: boolean;
  actions?: ReactNode;
}) {
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
        className={`border-rule bg-surface-card shadow-card relative max-h-[90vh] w-full overflow-y-auto rounded-t-lg border-t-[6px] border-t-accent p-5 sm:rounded-lg ${wide ? "max-w-[1100px] sm:p-7" : "max-w-[480px]"}`}
      >
        <div className="mb-4 flex items-start justify-between gap-3">
          <h2 className={`font-display font-extrabold tracking-[-0.02em] ${wide ? "text-[26px] sm:text-[30px]" : "text-[22px]"}`}>
            {title}
          </h2>
          {actions}
        </div>
        {children}
      </div>
    </div>
  );
}
