import { useState } from "react";
import { Link, useSearchParams } from "react-router";
import { Button, ErrorNote } from "../components/ui";
import { money, partPath, plural, qty, selectionSummary, selectionText, STAGE_COLOR, STAGE_LABEL, type Part, type SupplierGroup } from "../parts/format";
import { useMarkParts, useShoppingList } from "../parts/queries";
import { projectColor } from "../theme/resolve";

type Tab = "need" | "way" | "arrived";

const tabClass = (on: boolean) =>
  `label-mono flex min-h-11 shrink-0 cursor-pointer items-center gap-2 border-x-0 border-t-0 border-b-2 px-2 text-[12px] tracking-[0.12em] ${on ? "border-accent text-text" : "text-text-quiet hover:text-text border-transparent"}`;

/** What the selection bar can do on each tab. */
const ACTIONS: Record<Tab, { stage: string; label: string }[]> = {
  need: [{ stage: "ordered", label: "Mark ordered" }],
  way: [
    { stage: "shipped", label: "Mark shipped" },
    { stage: "arrived", label: "Mark arrived" },
  ],
  arrived: [{ stage: "fitted", label: "Mark fitted" }],
};

/** One list of parts to buy across every project, grouped by supplier. */
export function ShoppingPage() {
  const list = useShoppingList();
  const mark = useMarkParts();
  const [params, setParams] = useSearchParams();
  const tab = (["need", "way", "arrived"].includes(params.get("tab") ?? "") ? params.get("tab") : "need") as Tab;
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [done, setDone] = useState<string | null>(null);
  const data = list.data;

  const setTab = (t: Tab) => {
    setPicked(new Set());
    setParams((p) => (t === "need" ? p.delete("tab") : p.set("tab", t), p), { replace: true });
  };
  const toggle = (id: string) =>
    setPicked((s) => {
      const next = new Set(s);
      if (next.has(id)) next.delete(id);
      else next.add(id);
      return next;
    });

  const groups: SupplierGroup[] = !data ? [] : tab === "need" ? data.need : tab === "way" ? data.on_the_way : [];
  const shown: Part[] = !data ? [] : tab === "arrived" ? data.arrived : groups.flatMap((g) => g.parts);
  const selected = shown.filter((p) => picked.has(p.row_id));
  const needCount = data?.need.reduce((n, g) => n + g.parts.length, 0) ?? 0;
  const wayCount = data?.on_the_way.reduce((n, g) => n + g.parts.length, 0) ?? 0;

  const run = (stage: string) =>
    mark.mutate(
      { row_ids: selected.map((p) => p.row_id), stage, date: data?.date },
      {
        onSuccess: (r) => {
          setPicked(new Set());
          const ready = r.tasks_ready.map((t) => `T-${t.number} ${t.title}`);
          setDone(
            `${plural(r.moved, "part")} marked ${STAGE_LABEL[stage]!.toLowerCase()}.` +
              (ready.length ? ` All parts are in for ${ready.join(", ")}.` : ""),
          );
        },
      },
    );

  return (
    <div className="mx-auto flex w-full max-w-[1400px] flex-col gap-5 px-4 pt-5 pb-6 sm:px-8 sm:pt-8 sm:pb-10">
      <header className="flex items-end gap-x-4 sm:gap-x-8">
        <div className="mr-auto min-w-0">
          <p className="label-mono text-text-quiet truncate text-[12px] tracking-[0.12em]">
            {data ? `${plural(needCount, "part")} · across ${plural(data.project_count, "project")}` : " "}
          </p>
          <h1 className="font-display text-[38px] leading-[1.05] font-extrabold tracking-[-0.03em] sm:text-[52px]">Shopping list</h1>
        </div>
        {data && (
          <>
            <Total label="To buy" value={money(data.to_buy, data.currency)} />
            <Total label="On the way" value={money(data.on_the_way_total, data.currency)} className="hidden sm:block" />
          </>
        )}
      </header>

      <nav aria-label="Parts by stage" className="border-rule flex gap-6 overflow-x-auto border-x-0 border-t-0">
        <button type="button" className={tabClass(tab === "need")} aria-current={tab === "need"} onClick={() => setTab("need")}>
          Need · {needCount}
        </button>
        <button type="button" className={tabClass(tab === "way")} aria-current={tab === "way"} onClick={() => setTab("way")}>
          On the way · {wayCount}
        </button>
        <button type="button" className={tabClass(tab === "arrived")} aria-current={tab === "arrived"} onClick={() => setTab("arrived")}>
          Arrived<span className="max-sm:hidden"> this month</span> · {data?.arrived.length ?? 0}
        </button>
      </nav>

      <ErrorNote error={list.error ?? mark.error} />
      {done && (
        <p role="status" className="border-rule text-text-quiet flex items-center gap-3 rounded-md px-3 py-2 text-sm">
          <span className="flex-1">{done}</span>
          <button type="button" className="label-mono hover:text-text cursor-pointer text-[11px]" onClick={() => setDone(null)}>
            OK
          </button>
        </p>
      )}
      {!data && !list.error && <p className="label-mono text-text-quiet">Loading…</p>}

      {data && (
        <div className="flex flex-col gap-5">
          {selected.length > 0 && (
            <div className="border-accent bg-surface-raised shadow-card sticky bottom-3 z-10 order-last flex items-center gap-x-4 gap-y-2 rounded-lg border px-4 py-3 sm:static sm:order-none sm:px-5">
              <p className="mr-auto min-w-0">
                <b className="text-[17px]">
                  {selected.length}
                  <span className="max-sm:hidden"> {selected.length === 1 ? "part" : "parts"}</span> selected ·{" "}
                  {money(selectionSummary(selected).total, data.currency)}
                </b>
                <span className="text-text-quiet ml-3 hidden sm:inline">{selectionText(selected)}</span>
              </p>
              {ACTIONS[tab].map((a, i) => (
                <Button
                  key={a.stage}
                  variant={i === ACTIONS[tab].length - 1 ? "primary" : "ghost"}
                  className={i === ACTIONS[tab].length - 1 ? "" : "border"}
                  disabled={mark.isPending}
                  onClick={() => run(a.stage)}
                >
                  {a.label}
                </Button>
              ))}
              <Button className="border max-sm:hidden" onClick={() => setPicked(new Set())}>
                Clear
              </Button>
            </div>
          )}

          {tab !== "arrived" && groups.length === 0 && <Empty tab={tab} />}
          {groups.map((g) => (
            <Group key={g.supplier || "-"} group={g} currency={data.currency} picked={picked} onToggle={toggle} />
          ))}
          {tab === "arrived" &&
            (data.arrived.length === 0 ? (
              <Empty tab={tab} />
            ) : (
              <section className="border-rule bg-surface-card shadow-card overflow-hidden rounded-lg">
                <ul>
                  {data.arrived.map((p) => (
                    <PartRow key={p.row_id} part={p} currency={data.currency} picked={picked.has(p.row_id)} onToggle={toggle} showDate />
                  ))}
                </ul>
              </section>
            ))}
        </div>
      )}
    </div>
  );
}

function Total({ label, value, className = "" }: { label: string; value: string; className?: string }) {
  return (
    <div className={`text-right ${className}`}>
      <p className="label-mono text-text-quiet text-[12px] tracking-[0.12em] max-sm:hidden">{label}</p>
      <p className="font-mono text-[22px] leading-tight sm:text-[34px]" aria-label={`${label}: ${value}`}>{value}</p>
    </div>
  );
}

function Empty({ tab }: { tab: Tab }) {
  const text =
    tab === "need"
      ? "Nothing to buy. Parts in Need show up here from every project's Parts database. Make one from a project's Data tab: + Database, then Parts."
      : tab === "way"
        ? "Nothing on the way. Tick parts under Need and press Mark ordered."
        : "Nothing has arrived this month yet.";
  return (
    <div className="border-rule max-w-xl rounded-lg border-dashed p-5">
      <p className="text-text-quiet leading-relaxed">{text}</p>
    </div>
  );
}

function Group({
  group,
  currency,
  picked,
  onToggle,
}: {
  group: SupplierGroup;
  currency: string;
  picked: Set<string>;
  onToggle: (id: string) => void;
}) {
  const [copied, setCopied] = useState(false);
  const links = group.parts.map((p) => p.link).filter((l): l is string => !!l);
  const name = group.supplier || "No supplier";
  return (
    <section aria-label={name} className="border-rule bg-surface-card shadow-card overflow-hidden rounded-lg">
      <header className="border-rule flex min-h-16 flex-wrap items-center gap-x-4 gap-y-1 border-x-0 border-t-0 px-4 py-2.5 sm:px-5">
        <h2 className={`font-display text-[22px] font-extrabold tracking-[-0.02em] ${group.supplier ? "" : "text-text-quiet"}`}>{name}</h2>
        <span className="label-mono text-text-quiet text-[12px] tracking-[0.1em]">
          <span className="hidden sm:inline">{plural(group.parts.length, "part")} · </span>
          {money(group.total, currency)}
        </span>
        {links.length > 0 && (
          <Button
            className="ml-auto min-h-10 border px-3 text-sm max-sm:hidden"
            title="Copy every link in this group, one per line, to paste into the supplier's cart or a note"
            onClick={() => {
              void navigator.clipboard?.writeText(links.join("\n")).then(() => {
                setCopied(true);
                setTimeout(() => setCopied(false), 1500);
              });
            }}
          >
            {copied ? "Copied" : `Copy ${links.length === 1 ? "link" : `${links.length} links`}`}
          </Button>
        )}
      </header>
      <ul>
        {group.parts.map((p) => (
          <PartRow key={p.row_id} part={p} currency={currency} picked={picked.has(p.row_id)} onToggle={onToggle} />
        ))}
      </ul>
    </section>
  );
}

function PartRow({
  part,
  currency,
  picked,
  onToggle,
  showDate = false,
}: {
  part: Part;
  currency: string;
  picked: boolean;
  onToggle: (id: string) => void;
  showDate?: boolean;
}) {
  const color = projectColor(part.project_color);
  const forTask = part.tasks.map((t) => `T-${t.number}`).join(", ");
  const when = showDate && part.arrived_on ? new Date(`${part.arrived_on}T12:00`).toLocaleDateString("en-GB", { day: "numeric", month: "short" }) : null;
  return (
    <li className="border-rule flex min-h-[68px] items-center gap-3 border-x-0 border-t-0 px-4 py-2 last:border-b-0 sm:gap-5 sm:px-5">
      <input
        type="checkbox"
        aria-label={`Select ${part.item}`}
        checked={picked}
        disabled={!part.can_edit}
        onChange={() => onToggle(part.row_id)}
        className="accent-accent size-6 shrink-0 cursor-pointer"
      />
      <div className="min-w-0 flex-1 sm:flex sm:items-center sm:gap-5">
        <span className="flex min-w-0 items-center gap-2 sm:flex-1">
          <Link to={partPath(part)} className="truncate text-[17px] font-bold hover:underline">
            {part.item}
          </Link>
          {part.link && (
            <a href={part.link} target="_blank" rel="noreferrer noopener" className="text-text-quiet hover:text-accent shrink-0" aria-label={`Open the link for ${part.item}`}>
              ↗
            </a>
          )}
        </span>
        <span className="label-mono block truncate text-[11px] tracking-[0.1em] sm:w-56 sm:shrink-0" style={{ color }}>
          {part.project_title}
          {forTask && <span className="text-text-quiet"> · {forTask}</span>}
          <span className="text-text-quiet sm:hidden"> · {qty(part.qty)}</span>
        </span>
      </div>
      <span className="text-text-quiet hidden w-12 shrink-0 text-right font-mono text-[14px] sm:block">{qty(part.qty)}</span>
      <span className="w-20 shrink-0 text-right font-mono text-[15px]">{part.unit_cost == null ? "–" : money(part.total, currency)}</span>
      <span className="label-mono hidden w-20 shrink-0 text-right text-[11px] tracking-[0.1em] sm:block" style={{ color: STAGE_COLOR[part.stage] }}>
        {when ?? STAGE_LABEL[part.stage]}
      </span>
    </li>
  );
}
