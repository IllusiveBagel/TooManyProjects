import type { CSSProperties } from "react";
import { useMe } from "../auth/queries";

/** The home canvas. Phase 1 replaces the empty state with React Flow. */
export function CanvasPage() {
  const me = useMe();
  return (
    <main className="canvas-grid relative flex flex-1 items-center justify-center overflow-hidden p-4">
      <div className="border-rule bg-surface-card shadow-card w-full max-w-[320px] rounded-lg border-t-[6px] border-t-slot-1 p-4">
        <div className="flex items-center gap-2">
          <span className="label-mono text-slot-1 font-semibold">Home</span>
          <span className="label-mono border-accent text-accent ml-auto rounded-sm border px-1.5 py-0.5 text-[10px]">Empty</span>
        </div>
        <h1 className="font-display mt-2.5 text-[22px] leading-[1.05] font-extrabold tracking-[-0.02em]">
          Welcome in, {me.name.split(" ")[0]}
        </h1>
        <p className="text-text-quiet mt-2.5 text-sm leading-relaxed">
          Your canvas is ready. Projects, links between them and the board come next.
        </p>
        <div className="gauge mt-3" style={{ "--gauge-value": 0, "--gauge-color": "var(--tmp-slot-1)" } as CSSProperties} />
        <div className="label-mono text-text-quiet mt-2.5 flex justify-between">
          <span>0/0</span>
          <span>Phase 0</span>
        </div>
      </div>
    </main>
  );
}
