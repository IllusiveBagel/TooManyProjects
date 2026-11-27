// A whiteboard inside a note, from `![[Wiring plan]]`: the whole board,
// fitted, with a link to open it.
import { useMemo } from "react";
import { Link } from "react-router";
import { ViaBadge } from "../components/ui";
import { ago } from "../notes/format";
import { fromData, resolvedFonts, sortByZ, type El } from "./model";
import { useBoardPreview, type Whiteboard } from "./queries";
import { Preview } from "./Scene";

export function WhiteboardEmbed({ board }: { board: Whiteboard }) {
  const q = useBoardPreview(board.id, board.seq);
  const fonts = useMemo(() => resolvedFonts(), []);
  const els = useMemo(
    () =>
      sortByZ(
        (q.data?.elements ?? [])
          .map((s) => fromData(s.id, s.kind, s.data as Record<string, unknown>))
          .filter((e): e is El => !!e),
      ),
    [q.data],
  );
  const href = `/projects/${board.project_id}/whiteboards/${board.id}`;
  return (
    <section
      aria-label={`Embedded whiteboard: ${board.title}`}
      className="border-rule bg-surface-card shadow-card not-prose my-6 overflow-hidden rounded-lg font-sans text-[14px] leading-normal"
    >
      <header className="border-rule bg-surface-raised flex flex-wrap items-center gap-2.5 border-x-0 border-t-0 px-3 py-2">
        <span className="font-display text-base font-extrabold">{board.title}</span>
        <span className="label-mono text-text-quiet text-[10.5px]">
          Whiteboard · {board.project_title} · edited {ago(board.updated_at)}
          <ViaBadge via={board.updated_via} />
        </span>
        <Link
          to={href}
          className="label-mono border-rule hover:bg-surface ml-auto inline-flex min-h-8 items-center rounded-sm px-2 text-[10.5px]"
        >
          Open ↗
        </Link>
      </header>
      <Link to={href} className="bg-surface block h-72 sm:h-96" aria-label={`Open ${board.title}`}>
        {els.length ? (
          <Preview els={els} measure={fonts} grid />
        ) : (
          <span className="label-mono text-text-quiet grid h-full place-items-center text-[11px]">
            {q.data ? "Nothing on it yet" : "Loading…"}
          </span>
        )}
      </Link>
    </section>
  );
}
