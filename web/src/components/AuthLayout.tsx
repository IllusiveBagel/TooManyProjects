import type { ReactNode } from "react";
import { Wordmark } from "./ui";

/** Centred card on graph paper, for setup, login and invites. */
export function AuthLayout({ title, intro, children }: { title: string; intro?: ReactNode; children: ReactNode }) {
  return (
    <main className="canvas-grid flex min-h-screen items-center justify-center px-4 py-10">
      <div className="w-full max-w-[400px]">
        <div className="mb-6 flex justify-center">
          <Wordmark />
        </div>
        <div className="border-rule bg-surface-card shadow-card rounded-lg border-t-[6px] border-t-accent p-6">
          <h1 className="font-display text-[26px] leading-tight font-extrabold tracking-[-0.02em]">{title}</h1>
          {intro && <p className="text-text-quiet mt-2 text-sm leading-relaxed">{intro}</p>}
          <div className="mt-6">{children}</div>
        </div>
      </div>
    </main>
  );
}
