import type { components } from "../api/schema";

export type Attachment = components["schemas"]["Attachment"];

/** Where a file's bytes are: the original, or an image's 400 px or 1600 px preview. */
export function fileUrl(a: Pick<Attachment, "id">, size?: 400 | 1600, download = false): string {
  const q = new URLSearchParams();
  if (size) q.set("size", String(size));
  if (download) q.set("download", "true");
  const s = q.toString();
  return `/api/v1/attachments/${a.id}/file${s ? `?${s}` : ""}`;
}

/** A note's embedded file, found by name the way Obsidian finds it. */
export function noteFileUrl(noteId: string, name: string, size?: 400 | 1600): string {
  const q = new URLSearchParams({ name });
  if (size) q.set("size", String(size));
  return `/api/v1/notes/${noteId}/file?${q}`;
}

/** 2.4 MB, 18 MB, 3 KB. */
export function fileSize(bytes: number): string {
  if (bytes >= 1024 * 1024) {
    const mb = bytes / (1024 * 1024);
    return `${mb < 10 ? mb.toFixed(1) : Math.round(mb)} MB`;
  }
  if (bytes >= 1024) return `${Math.round(bytes / 1024)} KB`;
  return `${bytes} bytes`;
}

/** The file's type as a short label: PDF, STL, ZIP. */
export function fileKind(a: Pick<Attachment, "name" | "mime">): string {
  const ext = /\.([a-z0-9]{1,8})$/i.exec(a.name)?.[1];
  if (ext) return ext.toUpperCase();
  const sub = a.mime.split("/")[1] ?? "";
  return sub && sub !== "octet-stream" ? sub.toUpperCase() : "FILE";
}

/** When the photo was taken, else when it was added: what a build log sorts by. */
export function when(a: Pick<Attachment, "taken_at" | "created_at">): Date {
  const t = a.taken_at;
  // EXIF time has no zone: it is the camera's local time.
  return t ? new Date(t.length === 19 ? t : t.replace(/Z$/, "")) : new Date(a.created_at);
}

/** 9 Nov. */
export function shortDay(d: Date): string {
  return d.toLocaleDateString("en-GB", { day: "numeric", month: "short" });
}

/** The label on a tile: the caption, else the task (T-5 Strip interior) or note, else the file name. */
export function tileLabel(a: Attachment, withSource = true): string {
  const source = a.task_number
    ? `T-${a.task_number} ${a.task_title ?? ""}`.trim()
    : a.note_title
      ? `Note: ${a.note_title}`
      : "";
  if (a.caption) return withSource && a.task_number ? `T-${a.task_number} ${a.caption}` : a.caption;
  return withSource && source ? source : a.name.replace(/\.[a-z0-9]+$/i, "");
}

const startOfWeek = (d: Date) => {
  const day = new Date(d.getFullYear(), d.getMonth(), d.getDate());
  // Weeks start on Monday.
  day.setDate(day.getDate() - ((day.getDay() + 6) % 7));
  return day;
};

export interface WeekGroup {
  /** This week, Last week, or the week's dates. */
  title: string;
  /** 9 to 15 Nov. */
  range: string;
  items: Attachment[];
}

/** Files grouped by the week they were taken or added, newest week first. */
export function byWeek(items: Attachment[], now = new Date()): WeekGroup[] {
  const thisWeek = startOfWeek(now).getTime();
  const groups = new Map<number, Attachment[]>();
  for (const a of [...items].sort((x, y) => when(y).getTime() - when(x).getTime())) {
    const key = startOfWeek(when(a)).getTime();
    groups.set(key, [...(groups.get(key) ?? []), a]);
  }
  return [...groups.entries()]
    .sort((a, b) => b[0] - a[0])
    .map(([start, list]) => {
      const from = new Date(start);
      const to = new Date(start);
      to.setDate(to.getDate() + 6);
      const sameMonth = from.getMonth() === to.getMonth();
      const range = sameMonth
        ? `${from.getDate()} to ${shortDay(to)}`
        : `${shortDay(from)} to ${shortDay(to)}`;
      const weeks = Math.round((thisWeek - start) / (7 * 86_400_000));
      const title =
        weeks === 0
          ? "This week"
          : weeks === 1
            ? "Last week"
            : from.getFullYear() === now.getFullYear()
              ? `Week of ${shortDay(from)}`
              : `Week of ${shortDay(from)} ${from.getFullYear()}`;
      return { title, range, items: list };
    });
}

/** Files from a paste or drop, if it carried any. */
export function filesFrom(data: DataTransfer | null): File[] {
  if (!data) return [];
  return Array.from(data.files ?? []);
}
